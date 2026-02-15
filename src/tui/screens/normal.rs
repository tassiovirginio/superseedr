// SPDX-FileCopyrightText: 2025 The superseedr Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::app::AppCommand;
use crate::app::FileBrowserMode;
use crate::app::{App, AppMode, AppState, ConfigItem, SelectedHeader, TorrentControlState};
use crate::config::Settings;
use crate::config::SortDirection;
use crate::theme::ThemeContext;
use crate::torrent_manager::ManagerCommand;
use crate::tui::events::{handle_navigation, handle_pasted_text};
use crate::tui::formatters::{
    format_bytes, format_countdown, format_duration, format_speed, ip_to_color, parse_peer_id,
    sanitize_text, speed_to_style, truncate_with_ellipsis,
};
use crate::tui::layout::calculate_layout;
use crate::tui::layout::compute_smart_table_layout;
use crate::tui::layout::compute_visible_peer_columns;
use crate::tui::layout::compute_visible_torrent_columns;
use crate::tui::layout::get_peer_columns;
use crate::tui::layout::get_torrent_columns;
use crate::tui::layout::LayoutContext;
use crate::tui::layout::{PeerColumnId, SmartCol};
use crate::app::torrent_completion_percent;
use crate::app::PeerInfo;

#[cfg(windows)]
use clipboard::{ClipboardContext, ClipboardProvider};
use ratatui::crossterm::event::{Event as CrosstermEvent, KeyCode, KeyEventKind};
use ratatui::layout::Layout;
use ratatui::prelude::{
    symbols, Alignment, Constraint, Direction, Frame, Line, Modifier, Rect, Span, Style,
    Stylize,
};
use ratatui::widgets::{
    Block, Borders, Cell, Clear, Gauge, LineGauge, Padding, Paragraph, Row, Table, TableState,
    Wrap,
};
use strum::IntoEnumIterator;
#[cfg(windows)]
use tracing::{event as tracing_event, Level};

static APP_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn draw_status_error_popup(f: &mut Frame, error_text: &str, ctx: &ThemeContext) {
    let popup_width_percent: u16 = 50;
    let popup_height: u16 = 8;
    let vertical_chunks = ratatui::layout::Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(popup_height),
        Constraint::Min(0),
    ])
    .split(f.area());
    let area = ratatui::layout::Layout::horizontal([
        Constraint::Percentage((100 - popup_width_percent) / 2),
        Constraint::Percentage(popup_width_percent),
        Constraint::Percentage((100 - popup_width_percent) / 2),
    ])
    .split(vertical_chunks[1])[1];

    f.render_widget(Clear, area);
    let text = vec![
        Line::from(Span::styled(
            "Error",
            ctx.apply(Style::default().fg(ctx.state_error()).bold()),
        )),
        Line::from(""),
        Line::from(Span::styled(
            error_text,
            ctx.apply(Style::default().fg(ctx.state_warning())),
        )),
        Line::from(""),
        Line::from(""),
        Line::from(Span::styled(
            "[Press Esc to dismiss]",
            ctx.apply(Style::default().fg(ctx.theme.semantic.subtext1)),
        )),
    ];
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(ctx.apply(Style::default().fg(ctx.state_error())));
    let paragraph = Paragraph::new(text)
        .block(block)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
    f.render_widget(paragraph, area);
}

pub fn draw_shutdown_screen(f: &mut Frame, app_state: &AppState, ctx: &ThemeContext) {
    const POPUP_WIDTH: u16 = 40;
    const POPUP_HEIGHT: u16 = 3;
    let area = f.area();
    let width = POPUP_WIDTH.min(area.width);
    let height = POPUP_HEIGHT.min(area.height);
    let vertical_chunks = ratatui::layout::Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(height),
        Constraint::Min(0),
    ])
    .split(area);
    let area = ratatui::layout::Layout::horizontal([
        Constraint::Min(0),
        Constraint::Length(width),
        Constraint::Min(0),
    ])
    .split(vertical_chunks[1])[1];

    f.render_widget(Clear, area);
    let container_block = Block::default()
        .title(Span::styled(
            " Exiting ",
            ctx.apply(Style::default().fg(ctx.accent_peach())),
        ))
        .borders(Borders::ALL)
        .border_style(ctx.apply(Style::default().fg(ctx.theme.semantic.border)));
    let inner_area = container_block.inner(area);
    f.render_widget(container_block, area);

    let chunks = ratatui::layout::Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1)])
        .split(inner_area);
    let progress_label = format!("{:.0}%", (app_state.shutdown_progress * 100.0).min(100.0));
    let progress_bar = Gauge::default()
        .ratio(app_state.shutdown_progress)
        .label(progress_label)
        .gauge_style(
            ctx.apply(
                Style::default()
                    .fg(ctx.state_selected())
                    .bg(ctx.theme.semantic.surface0),
            ),
        );
    f.render_widget(progress_bar, chunks[0]);
}

pub(crate) fn truncate_theme_label_preserving_fx(
    theme_name: &str,
    fx_enabled: bool,
    max_len: usize,
) -> String {
    if max_len == 0 {
        return String::new();
    }

    if !fx_enabled {
        return truncate_with_ellipsis(theme_name, max_len);
    }

    let suffix = "[FX]";
    let suffix_len = suffix.chars().count();
    let full = format!("{theme_name} {suffix}");
    if full.chars().count() <= max_len {
        return full;
    }

    if max_len <= 3 {
        return ".".repeat(max_len);
    }

    if max_len <= suffix_len + 3 {
        return truncate_with_ellipsis(&full, max_len);
    }

    let name_len = max_len.saturating_sub(3 + suffix_len);
    let name_prefix: String = theme_name.chars().take(name_len).collect();
    format!("{name_prefix}...{suffix}")
}

pub(crate) fn compute_footer_left_width(footer_width: u16, is_update: bool) -> u16 {
    let min_left = if is_update { 68u16 } else { 48u16 };
    let max_left = if is_update { 110u16 } else { 90u16 };
    let right_status = 21u16;
    let min_commands = 18u16;
    let reserved = right_status + min_commands;

    let available_for_left = footer_width.saturating_sub(reserved);
    available_for_left.clamp(min_left, max_left)
}

fn estimate_footer_left_content_width(app_state: &AppState, ctx: &ThemeContext) -> u16 {
    let fx_enabled = ctx.theme.effects.enabled();
    let theme_label = if fx_enabled {
        format!("{} [FX]", ctx.theme.name)
    } else {
        ctx.theme.name.to_string()
    };

    let content = if let Some(new_version) = &app_state.update_available {
        format!(
            "UPDATE AVAILABLE: v{} -> v{} | {} | {}",
            APP_VERSION,
            new_version,
            app_state.data_rate.to_string(),
            theme_label
        )
    } else {
        #[cfg(all(feature = "dht", feature = "pex"))]
        {
            format!(
                "superseedr v{} | {} | {}",
                APP_VERSION,
                app_state.data_rate.to_string(),
                theme_label
            )
        }
        #[cfg(not(all(feature = "dht", feature = "pex")))]
        {
            format!(
                "superseedr [PRIVATE] v{} | {} | {}",
                APP_VERSION,
                app_state.data_rate.to_string(),
                theme_label
            )
        }
    };

    (content.chars().count() as u16).saturating_add(2)
}

fn footer_command_len(key: &str, suffix: &str) -> usize {
    key.chars().count() + suffix.chars().count()
}

fn try_push_footer_command(
    spans: &mut Vec<Span<'static>>,
    used_width: &mut usize,
    max_width: usize,
    key: &'static str,
    suffix: &'static str,
    key_style: Style,
) -> bool {
    let item_width = footer_command_len(key, suffix);
    let separator_width = if *used_width == 0 { 0 } else { 3 };
    if *used_width + separator_width + item_width > max_width {
        return false;
    }

    if separator_width > 0 {
        spans.push(Span::raw(" | "));
    }
    spans.push(Span::styled(key, key_style));
    spans.push(Span::raw(suffix));
    *used_width += separator_width + item_width;
    true
}

pub fn draw_footer(
    f: &mut Frame,
    app_state: &AppState,
    settings: &Settings,
    footer_chunk: ratatui::layout::Rect,
    ctx: &ThemeContext,
) {
    let show_branding = footer_chunk.width >= 80;

    let is_update = app_state.update_available.is_some();
    let (left_constraint, right_constraint) = if show_branding {
        let min_left = if is_update { 52u16 } else { 40u16 };
        let min_commands = 18u16;
        let desired_left = compute_footer_left_width(footer_chunk.width, is_update);
        let content_left = estimate_footer_left_content_width(app_state, ctx);
        let left_target = desired_left.min(content_left.max(min_left));
        let symmetric_left_cap = footer_chunk.width.saturating_sub(min_commands) / 2;

        if symmetric_left_cap >= min_left {
            let symmetric_left = left_target.min(symmetric_left_cap);
            (
                Constraint::Length(symmetric_left),
                Constraint::Length(symmetric_left),
            )
        } else {
            (Constraint::Length(left_target), Constraint::Length(21))
        }
    } else {
        (Constraint::Length(0), Constraint::Length(21))
    };

    let footer_layout = ratatui::layout::Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            left_constraint,
            Constraint::Min(0),
            right_constraint,
        ])
        .split(footer_chunk);

    let client_id_chunk = footer_layout[0];
    let commands_chunk = footer_layout[1];
    let status_chunk = footer_layout[2];

    if show_branding {
        let current_dl_speed = *app_state.avg_download_history.last().unwrap_or(&0);
        let current_ul_speed = *app_state.avg_upload_history.last().unwrap_or(&0);
        let fx_enabled = ctx.theme.effects.enabled();
        let theme_name = ctx.theme.name.to_string();
        let fit_theme_label = |prefix: &str| -> String {
            let max_theme_width =
                (client_id_chunk.width as usize).saturating_sub(prefix.chars().count());
            if max_theme_width == 0 {
                String::new()
            } else if max_theme_width <= 3 {
                ".".repeat(max_theme_width)
            } else {
                truncate_theme_label_preserving_fx(&theme_name, fx_enabled, max_theme_width)
            }
        };

        let client_display_line = if let Some(new_version) = &app_state.update_available {
            let theme_display = fit_theme_label(&format!(
                "UPDATE AVAILABLE: v{} -> v{} | {} | ",
                APP_VERSION,
                new_version,
                app_state.data_rate.to_string()
            ));
            Line::from(vec![
                Span::styled(
                    "UPDATE AVAILABLE: ",
                    ctx.apply(Style::default().fg(ctx.state_warning()).bold()),
                ),
                Span::styled(
                    format!("v{}", APP_VERSION),
                    Style::default()
                        .fg(ctx.theme.semantic.surface2)
                        .add_modifier(ratatui::prelude::Modifier::CROSSED_OUT),
                ),
                Span::styled(
                    " \u{2192} ",
                    ctx.apply(Style::default().fg(ctx.theme.semantic.surface2)),
                ),
                Span::styled(
                    format!("v{}", new_version),
                    ctx.apply(Style::default().fg(ctx.state_success()).bold()),
                ),
                Span::styled(
                    " | ",
                    ctx.apply(Style::default().fg(ctx.theme.semantic.surface2)),
                ),
                Span::styled(
                    app_state.data_rate.to_string(),
                    ctx.apply(Style::default().fg(ctx.theme.semantic.subtext1)),
                ),
                Span::styled(
                    " | ",
                    ctx.apply(Style::default().fg(ctx.theme.semantic.surface2)),
                ),
                Span::styled(
                    theme_display,
                    ctx.apply(Style::default().fg(ctx.state_selected())),
                ),
            ])
        } else {
            #[cfg(all(feature = "dht", feature = "pex"))]
            {
                let theme_display = fit_theme_label(&format!(
                    "superseedr v{} | {} | ",
                    APP_VERSION,
                    app_state.data_rate.to_string()
                ));
                Line::from(vec![
                    Span::styled(
                        "super",
                        ctx.apply(speed_to_style(ctx, current_dl_speed).add_modifier(
                            ratatui::prelude::Modifier::BOLD,
                        )),
                    ),
                    Span::styled(
                        "seedr",
                        ctx.apply(speed_to_style(ctx, current_ul_speed).add_modifier(
                            ratatui::prelude::Modifier::BOLD,
                        )),
                    ),
                    Span::styled(
                        format!(" v{}", APP_VERSION),
                        ctx.apply(Style::default().fg(ctx.theme.semantic.subtext1)),
                    ),
                    Span::styled(
                        " | ",
                        ctx.apply(Style::default().fg(ctx.theme.semantic.surface2)),
                    ),
                    Span::styled(
                        app_state.data_rate.to_string(),
                        ctx.apply(Style::default().fg(ctx.state_warning()).bold()),
                    ),
                    Span::styled(
                        " | ",
                        ctx.apply(Style::default().fg(ctx.theme.semantic.surface2)),
                    ),
                    Span::styled(
                        theme_display,
                        ctx.apply(Style::default().fg(ctx.state_selected())),
                    ),
                ])
            }
            #[cfg(not(all(feature = "dht", feature = "pex")))]
            {
                let theme_display = fit_theme_label(&format!(
                    "superseedr [PRIVATE] v{} | {} | ",
                    APP_VERSION,
                    app_state.data_rate.to_string()
                ));
                Line::from(vec![
                    Span::styled(
                        "superseedr",
                        ctx.apply(Style::default().fg(ctx.theme.semantic.surface2)),
                    )
                    .add_modifier(ratatui::prelude::Modifier::CROSSED_OUT),
                    Span::styled(
                        " [PRIVATE]",
                        Style::default()
                            .fg(ctx.state_error())
                            .add_modifier(ratatui::prelude::Modifier::BOLD),
                    ),
                    Span::styled(
                        format!(" v{}", APP_VERSION),
                        ctx.apply(Style::default().fg(ctx.theme.semantic.subtext1)),
                    ),
                    Span::styled(
                        " | ",
                        ctx.apply(Style::default().fg(ctx.theme.semantic.surface2)),
                    ),
                    Span::styled(
                        app_state.data_rate.to_string(),
                        ctx.apply(Style::default().fg(ctx.state_warning()).bold()),
                    ),
                    Span::styled(
                        " | ",
                        ctx.apply(Style::default().fg(ctx.theme.semantic.surface2)),
                    ),
                    Span::styled(
                        theme_display,
                        ctx.apply(Style::default().fg(ctx.state_selected())),
                    ),
                ])
            }
        };

        let client_id_paragraph = Paragraph::new(client_display_line).alignment(Alignment::Left);
        f.render_widget(client_id_paragraph, client_id_chunk);
    }

    let max_width = commands_chunk.width as usize;
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut used_width = 0usize;

    let manual_key = "[m]";
    let manual_suffix = if app_state.system_warning.is_some() {
        "anual (warning)"
    } else {
        "anual"
    };
    let manual_min_width = footer_command_len(manual_key, "");

    let mut push_if_fits = |key: &'static str, suffix: &'static str, key_style: Style| {
        let separator_width = if used_width == 0 { 0 } else { 3 };
        let candidate_width = footer_command_len(key, suffix);
        let required_for_manual = if used_width + separator_width + candidate_width == 0 {
            manual_min_width
        } else {
            3 + manual_min_width
        };
        if used_width + separator_width + candidate_width + required_for_manual <= max_width {
            let _ =
                try_push_footer_command(&mut spans, &mut used_width, max_width, key, suffix, key_style);
        }
    };

    push_if_fits(
        "Arrows",
        " nav",
        ctx.apply(Style::default().fg(ctx.state_info())),
    );
    push_if_fits(
        "[Q]",
        "uit",
        ctx.apply(Style::default().fg(ctx.state_error())),
    );
    push_if_fits(
        "[v]",
        "paste",
        ctx.apply(Style::default().fg(ctx.accent_teal())),
    );
    push_if_fits(
        "[p]",
        "ause",
        ctx.apply(Style::default().fg(ctx.state_success())),
    );
    push_if_fits(
        "[a]",
        "dd",
        ctx.apply(Style::default().fg(ctx.state_success())),
    );
    push_if_fits(
        "[d]",
        "elete",
        ctx.apply(Style::default().fg(ctx.state_warning())),
    );
    push_if_fits(
        "[s]",
        "ort",
        ctx.apply(Style::default().fg(ctx.state_selected())),
    );
    push_if_fits(
        "[t]",
        "ime",
        ctx.apply(Style::default().fg(ctx.accent_sapphire())),
    );
    push_if_fits(
        "[<]theme[>]",
        "",
        ctx.apply(Style::default().fg(ctx.state_selected())),
    );
    push_if_fits(
        "[/]",
        "search",
        ctx.apply(Style::default().fg(ctx.state_warning())),
    );
    push_if_fits(
        "[c]",
        "onfig",
        ctx.apply(Style::default().fg(ctx.state_complete())),
    );
    push_if_fits(
        "[d]",
        "elete",
        ctx.apply(Style::default().fg(ctx.state_error())),
    );
    push_if_fits(
        "[x]",
        "anon",
        ctx.apply(Style::default().fg(ctx.accent_sapphire())),
    );
    push_if_fits(
        "[z]",
        "power",
        ctx.apply(Style::default().fg(ctx.state_warning())),
    );
    push_if_fits(
        "[T]",
        "time++",
        ctx.apply(Style::default().fg(ctx.accent_sapphire())),
    );
    push_if_fits(
        "[[]",
        "slower",
        ctx.apply(Style::default().fg(ctx.state_info())),
    );
    push_if_fits(
        "[]]",
        "faster",
        ctx.apply(Style::default().fg(ctx.state_success())),
    );

    if !try_push_footer_command(
        &mut spans,
        &mut used_width,
        max_width,
        manual_key,
        manual_suffix,
        ctx.apply(Style::default().fg(ctx.accent_teal())),
    ) {
        let _ = try_push_footer_command(
            &mut spans,
            &mut used_width,
            max_width,
            manual_key,
            "anual",
            ctx.apply(Style::default().fg(ctx.accent_teal())),
        );
    }
    if !spans.iter().any(|s| matches!(s.content.as_ref(), "[m]")) {
        let _ = try_push_footer_command(
            &mut spans,
            &mut used_width,
            max_width,
            manual_key,
            "",
            ctx.apply(Style::default().fg(ctx.accent_teal())),
        );
    }

    let footer_paragraph = Paragraph::new(Line::from(spans))
        .alignment(Alignment::Center)
        .style(ctx.apply(Style::default().fg(ctx.theme.semantic.subtext1)));
    f.render_widget(footer_paragraph, commands_chunk);

    let port_style = if app_state.externally_accessable_port {
        ctx.apply(Style::default().fg(ctx.state_success()))
    } else {
        ctx.apply(Style::default().fg(ctx.state_error()))
    };
    let port_text = if app_state.externally_accessable_port {
        "Open"
    } else {
        "Closed"
    };

    let footer_status = Line::from(vec![
        Span::raw("Port: "),
        Span::styled(settings.client_port.to_string(), port_style),
        Span::raw(" ["),
        Span::styled(port_text, port_style),
        Span::raw("]"),
    ])
    .alignment(Alignment::Right);

    let status_paragraph =
        Paragraph::new(footer_status).style(ctx.apply(Style::default().fg(ctx.theme.semantic.subtext1)));
    f.render_widget(status_paragraph, status_chunk);
}

pub fn draw_torrent_list(f: &mut Frame, app_state: &AppState, area: Rect, ctx: &ThemeContext) {
    let mut table_state = TableState::default();
    if matches!(app_state.selected_header, SelectedHeader::Torrent(_)) {
        table_state.select(Some(app_state.selected_torrent_index));
    }

    let all_cols = get_torrent_columns();
    let (constraints, visible_indices) = compute_visible_torrent_columns(app_state, area.width);

    let (sort_col, sort_dir) = app_state.torrent_sort;
    let header_cells: Vec<Cell> = visible_indices
        .iter()
        .enumerate()
        .map(|(visual_idx, &real_idx)| {
            let def = &all_cols[real_idx];
            let is_selected = app_state.selected_header == SelectedHeader::Torrent(visual_idx);
            let is_sorting = def.sort_enum == Some(sort_col);

            let mut style = ctx.apply(Style::default().fg(ctx.state_warning()));
            if is_sorting {
                style = style.fg(ctx.state_selected());
            }
            style = ctx.apply(style);

            let mut spans = vec![];
            let mut text_span = Span::styled(def.header, style);
            if is_selected {
                text_span = text_span.underlined().bold();
            }
            spans.push(text_span);

            if is_sorting {
                let arrow = if sort_dir == SortDirection::Ascending {
                    " ▲"
                } else {
                    " ▼"
                };
                spans.push(Span::styled(arrow, style));
            }
            Cell::from(Line::from(spans))
        })
        .collect();
    let header = Row::new(header_cells).height(1);

    let rows =
        app_state
            .torrent_list_order
            .iter()
            .enumerate()
            .map(|(i, info_hash)| match app_state.torrents.get(info_hash) {
                Some(torrent) => {
                    let state = &torrent.latest_state;
                    let is_selected = i == app_state.selected_torrent_index;

                    let mut row_style = match state.torrent_control_state {
                        TorrentControlState::Running => {
                            ctx.apply(Style::default().fg(ctx.theme.semantic.text))
                        }
                        TorrentControlState::Paused => {
                            ctx.apply(Style::default().fg(ctx.theme.semantic.surface1))
                        }
                        TorrentControlState::Deleting => {
                            ctx.apply(Style::default().fg(ctx.state_error()))
                        }
                    };
                    row_style = ctx.apply(row_style);

                    if is_selected {
                        let is_safe_ascii = state.torrent_name.is_ascii();
                        if is_safe_ascii {
                            row_style = row_style.add_modifier(Modifier::BOLD);
                        }
                    }

                    let cells: Vec<Cell> = visible_indices
                        .iter()
                        .map(|&real_idx| {
                            let def = &all_cols[real_idx];
                            match def.id {
                                crate::tui::layout::ColumnId::Status => {
                                    let display_pct = torrent_completion_percent(state);
                                    Cell::from(format!("{:.1}%", display_pct))
                                }
                                crate::tui::layout::ColumnId::Name => {
                                    let name = if app_state.anonymize_torrent_names {
                                        format!("Torrent {}", i + 1)
                                    } else {
                                        sanitize_text(&state.torrent_name)
                                    };
                                    let mut c = Cell::from(name);
                                    if is_selected {
                                        let s = ctx.apply(
                                            ctx.apply(Style::default().fg(ctx.state_warning())),
                                        );
                                        c = c.style(s);
                                    }
                                    c
                                }
                                crate::tui::layout::ColumnId::DownSpeed => {
                                    Cell::from(format_speed(torrent.smoothed_download_speed_bps))
                                        .style(ctx.apply(speed_to_style(
                                            ctx,
                                            torrent.smoothed_download_speed_bps,
                                        )))
                                }
                                crate::tui::layout::ColumnId::UpSpeed => {
                                    Cell::from(format_speed(torrent.smoothed_upload_speed_bps))
                                        .style(ctx.apply(speed_to_style(
                                            ctx,
                                            torrent.smoothed_upload_speed_bps,
                                        )))
                                }
                            }
                        })
                        .collect();

                    Row::new(cells).style(row_style)
                }
                None => Row::new(vec![Cell::from("Error retrieving data")]),
            });

    let border_style = if matches!(app_state.selected_header, SelectedHeader::Torrent(_)) {
        ctx.apply(Style::default().fg(ctx.state_selected()))
    } else {
        ctx.apply(Style::default().fg(ctx.theme.semantic.surface2))
    };

    let mut title_spans = Vec::new();
    if app_state.is_searching {
        title_spans.push(Span::raw("Search: /"));
        title_spans.push(Span::styled(
            &app_state.search_query,
            ctx.apply(Style::default().fg(ctx.state_warning())),
        ));
    } else if !app_state.search_query.is_empty() {
        title_spans.push(Span::styled(
            format!("[{}] ", app_state.search_query),
            ctx.apply(
                Style::default()
                    .fg(ctx.theme.semantic.subtext1)
                    .add_modifier(Modifier::ITALIC),
            ),
        ));
    }

    if !app_state.is_searching {
        if let Some(info_hash) = app_state
            .torrent_list_order
            .get(app_state.selected_torrent_index)
        {
            if let Some(torrent) = app_state.torrents.get(info_hash) {
                let path_cow;
                let text_to_show = if app_state.anonymize_torrent_names {
                    "/path/to/torrent/file"
                } else {
                    path_cow = torrent
                        .latest_state
                        .download_path
                        .as_ref()
                        .map(|p| p.to_string_lossy())
                        .unwrap_or_else(|| std::borrow::Cow::Borrowed("Unknown path"));
                    &sanitize_text(&path_cow)
                };

                let avail_width = area.width.saturating_sub(10) as usize;
                let display_name = truncate_with_ellipsis(text_to_show, avail_width);

                title_spans.push(Span::styled(
                    display_name,
                    ctx.apply(Style::default().fg(ctx.state_warning())),
                ));
            }
        }
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(Line::from(title_spans));

    let inner_area = block.inner(area);
    let table = Table::new(rows, constraints).header(header).block(block);
    f.render_stateful_widget(table, area, &mut table_state);

    if app_state.torrent_list_order.is_empty() {
        let empty_msg = vec![
            Line::from(Span::styled(
                "No Torrents",
                ctx.apply(
                    Style::default()
                        .fg(ctx.theme.semantic.surface2)
                        .add_modifier(Modifier::BOLD),
                ),
            )),
            Line::from(Span::styled(
                "Press [a] to add a file or [v] to paste a magnet link",
                ctx.apply(Style::default().fg(ctx.theme.semantic.surface2)),
            )),
        ];

        let center_y = inner_area.y + (inner_area.height / 2).saturating_sub(1);
        let text_area = Rect::new(inner_area.x, center_y, inner_area.width, 2);

        f.render_widget(
            Paragraph::new(empty_msg).alignment(Alignment::Center),
            text_area,
        );
    }
}

pub fn draw_details_panel(
    f: &mut Frame,
    app_state: &AppState,
    details_text_chunk: Rect,
    ctx: &ThemeContext,
) {
    let details_block = Block::default()
        .title(Span::styled(
            "Details",
            ctx.apply(Style::default().fg(ctx.state_selected())),
        ))
        .borders(Borders::ALL)
        .borders(Borders::ALL)
        .border_style(ctx.apply(Style::default().fg(ctx.theme.semantic.border)));
    let details_inner_chunk = details_block.inner(details_text_chunk);
    f.render_widget(details_block, details_text_chunk);

    let detail_rows = ratatui::layout::Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(details_inner_chunk);

    let selected_torrent = app_state
        .torrent_list_order
        .get(app_state.selected_torrent_index)
        .and_then(|h| app_state.torrents.get(h));

    if let Some(torrent) = selected_torrent {
        let state = &torrent.latest_state;

        let progress_chunks =
            ratatui::layout::Layout::horizontal([Constraint::Length(11), Constraint::Min(0)])
                .split(detail_rows[0]);

        f.render_widget(Paragraph::new("Progress: "), progress_chunks[0]);

        let (progress_ratio, progress_label_text) = if state.number_of_pieces_total > 0 {
            if state.torrent_control_state != TorrentControlState::Running
                || state.activity_message.contains("Seeding")
                || state.activity_message.contains("Finished")
            {
                (1.0, "100.0%".to_string())
            } else {
                let ratio =
                    state.number_of_pieces_completed as f64 / state.number_of_pieces_total as f64;
                (ratio, format!("{:.1}%", ratio * 100.0))
            }
        } else {
            (0.0, "0.0%".to_string())
        };
        let custom_line_set = symbols::line::Set {
            horizontal: "⣿",
            ..symbols::line::THICK
        };
        let line_gauge = LineGauge::default()
            .ratio(progress_ratio)
            .label(progress_label_text)
            .line_set(custom_line_set)
            .filled_style(ctx.apply(Style::default().fg(ctx.state_success())));
        f.render_widget(line_gauge, progress_chunks[1]);

        let status_text = if state.activity_message.is_empty() {
            "Waiting..."
        } else {
            state.activity_message.as_str()
        };
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    "Status:   ",
                    ctx.apply(Style::default().fg(ctx.theme.semantic.text)),
                ),
                Span::raw(status_text),
            ])),
            detail_rows[1],
        );

        let total_pieces = state.number_of_pieces_total as usize;
        let (seeds, leeches) = state
            .peers
            .iter()
            .filter(|p| p.last_action != "Connecting...")
            .fold((0, 0), |(s, l), peer| {
                if total_pieces > 0 {
                    let pieces_have = peer
                        .bitfield
                        .iter()
                        .take(total_pieces)
                        .filter(|&&b| b)
                        .count();
                    if pieces_have == total_pieces {
                        (s + 1, l)
                    } else {
                        (s, l + 1)
                    }
                } else {
                    (s, l + 1)
                }
            });
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    "Peers:    ",
                    ctx.apply(Style::default().fg(ctx.theme.semantic.text)),
                ),
                Span::raw(format!(
                    "{} (",
                    state.number_of_successfully_connected_peers
                )),
                Span::styled(
                    format!("{}", seeds),
                    ctx.apply(Style::default().fg(ctx.state_success())),
                ),
                Span::raw(" / "),
                Span::styled(
                    format!("{}", leeches),
                    ctx.apply(Style::default().fg(ctx.state_error())),
                ),
                Span::raw(")"),
            ])),
            detail_rows[2],
        );

        let written_size_spans = if state.number_of_pieces_completed < state.number_of_pieces_total
        {
            vec![
                Span::styled(
                    "Written:  ",
                    ctx.apply(Style::default().fg(ctx.theme.semantic.text)),
                ),
                Span::raw(format_bytes(state.bytes_written)),
                Span::raw(format!(" / {}", format_bytes(state.total_size))),
            ]
        } else {
            vec![
                Span::styled(
                    "Size:     ",
                    ctx.apply(Style::default().fg(ctx.theme.semantic.text)),
                ),
                Span::raw(format_bytes(state.total_size)),
            ]
        };
        f.render_widget(
            Paragraph::new(Line::from(written_size_spans)),
            detail_rows[3],
        );

        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    "Pieces:   ",
                    ctx.apply(Style::default().fg(ctx.theme.semantic.text)),
                ),
                Span::raw(format!(
                    "{}/{}",
                    state.number_of_pieces_completed, state.number_of_pieces_total
                )),
            ])),
            detail_rows[4],
        );

        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    "ETA:      ",
                    ctx.apply(Style::default().fg(ctx.theme.semantic.text)),
                ),
                Span::raw(format_duration(state.eta)),
            ])),
            detail_rows[5],
        );

        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    "Announce: ",
                    ctx.apply(Style::default().fg(ctx.theme.semantic.text)),
                ),
                Span::raw(format_countdown(state.next_announce_in)),
            ])),
            detail_rows[6],
        );
    } else {
        let placeholder_style = ctx.apply(Style::default().fg(ctx.theme.semantic.overlay0));
        let label_style = ctx.apply(Style::default().fg(ctx.theme.semantic.surface2));

        let progress_chunks =
            ratatui::layout::Layout::horizontal([Constraint::Length(11), Constraint::Min(0)])
                .split(detail_rows[0]);
        f.render_widget(
            Paragraph::new("Progress: ").style(label_style),
            progress_chunks[0],
        );
        let line_gauge = LineGauge::default()
            .ratio(0.0)
            .label(" --.--%")
            .style(placeholder_style);
        f.render_widget(line_gauge, progress_chunks[1]);

        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("Status:   ", label_style),
                Span::styled("No Selection", placeholder_style),
            ])),
            detail_rows[1],
        );

        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("Peers:    ", label_style),
                Span::styled("- (- / -)", placeholder_style),
            ])),
            detail_rows[2],
        );

        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("Size:     ", label_style),
                Span::styled("- / -", placeholder_style),
            ])),
            detail_rows[3],
        );

        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("Pieces:   ", label_style),
                Span::styled("- / -", placeholder_style),
            ])),
            detail_rows[4],
        );

        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("ETA:      ", label_style),
                Span::styled("--:--:--", placeholder_style),
            ])),
            detail_rows[5],
        );

        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("Announce: ", label_style),
                Span::styled("--s", placeholder_style),
            ])),
            detail_rows[6],
        );
    }
}

pub fn draw_peers_table(f: &mut Frame, app_state: &AppState, peers_chunk: Rect, ctx: &ThemeContext) {
    if peers_chunk.height < 2 || peers_chunk.width < 2 {
        return;
    }

    if let Some(info_hash) = app_state
        .torrent_list_order
        .get(app_state.selected_torrent_index)
    {
        if let Some(torrent) = app_state.torrents.get(info_hash) {
            let state = &torrent.latest_state;

            if peers_chunk.height > 0 {
                let has_established_peers =
                    state.peers.iter().any(|p| p.last_action != "Connecting...");

                let mut peers_to_display: Vec<PeerInfo> = if has_established_peers {
                    state
                        .peers
                        .iter()
                        .filter(|p| p.last_action != "Connecting...")
                        .cloned()
                        .collect()
                } else {
                    state.peers.clone()
                };

                let (sort_by, sort_direction) = app_state.peer_sort;
                peers_to_display.sort_by(|a, b| {
                    use crate::config::PeerSortColumn::*;
                    let ordering = match sort_by {
                        Flags => a.peer_choking.cmp(&b.peer_choking),
                        Completed => {
                            let total = state.number_of_pieces_total as usize;
                            if total == 0 {
                                std::cmp::Ordering::Equal
                            } else {
                                let a_c = a.bitfield.iter().take(total).filter(|&&h| h).count();
                                let b_c = b.bitfield.iter().take(total).filter(|&&h| h).count();
                                a_c.cmp(&b_c)
                            }
                        }
                        Address => a.address.cmp(&b.address),
                        Client => a.peer_id.cmp(&b.peer_id),
                        Action => a.last_action.cmp(&b.last_action),
                        DL => a.download_speed_bps.cmp(&b.download_speed_bps),
                        UL => a.upload_speed_bps.cmp(&b.upload_speed_bps),
                    };
                    if sort_direction == SortDirection::Ascending {
                        ordering
                    } else {
                        ordering.reverse()
                    }
                });

                let all_peer_cols = get_peer_columns();
                let smart_cols: Vec<SmartCol> = all_peer_cols
                    .iter()
                    .map(|c| SmartCol {
                        min_width: c.min_width,
                        priority: c.priority,
                        constraint: c.default_constraint,
                    })
                    .collect();

                let (constraints, visible_indices) =
                    compute_smart_table_layout(&smart_cols, peers_chunk.width, 1);

                let peer_border_style =
                    if matches!(app_state.selected_header, SelectedHeader::Peer(_)) {
                        ctx.apply(Style::default().fg(ctx.state_selected()))
                    } else {
                        ctx.apply(Style::default().fg(ctx.theme.semantic.surface2))
                    };

                if peers_to_display.is_empty() {
                    draw_swarm_heatmap(
                        f,
                        ctx,
                        &state.peers,
                        state.number_of_pieces_total,
                        peers_chunk,
                    );
                } else {
                    let header_cells: Vec<Cell> = visible_indices
                        .iter()
                        .enumerate()
                        .map(|(visual_idx, &real_idx)| {
                            let def = &all_peer_cols[real_idx];

                            let is_selected =
                                app_state.selected_header == SelectedHeader::Peer(visual_idx);
                            let is_sorting = def.sort_enum == Some(sort_by);

                            let mut style = ctx.apply(Style::default().fg(ctx.state_warning()));
                            if is_sorting {
                                style = style.fg(ctx.state_selected());
                            }
                            style = ctx.apply(style);

                            let mut text = def.header.to_string();
                            if is_sorting {
                                text.push_str(if sort_direction == SortDirection::Ascending {
                                    " ▲"
                                } else {
                                    " ▼"
                                });
                            }

                            let mut span = Span::styled(text, style);
                            if is_selected {
                                span = span.underlined().bold();
                            }
                            Cell::from(Line::from(vec![span]))
                        })
                        .collect();

                    let peer_header = Row::new(header_cells).height(1);

                    let peer_rows = peers_to_display.iter().map(|peer| {
                        let row_color =
                            if peer.download_speed_bps == 0 && peer.upload_speed_bps == 0 {
                                ctx.theme.semantic.surface1
                            } else {
                                ip_to_color(ctx, &peer.address)
                            };

                        let cells: Vec<Cell> = visible_indices
                            .iter()
                            .map(|&real_idx| {
                                let def = &all_peer_cols[real_idx];
                                match def.id {
                                    PeerColumnId::Flags => Line::from(vec![
                                        Span::styled(
                                            "■",
                                            ctx.apply(Style::default().fg(if peer.am_interested {
                                                ctx.accent_sapphire()
                                            } else {
                                                ctx.theme.semantic.surface1
                                            })),
                                        ),
                                        Span::styled(
                                            "■",
                                            ctx.apply(Style::default().fg(if peer.peer_choking {
                                                ctx.accent_maroon()
                                            } else {
                                                ctx.theme.semantic.surface1
                                            })),
                                        ),
                                        Span::styled(
                                            "■",
                                            ctx.apply(Style::default().fg(
                                                if peer.peer_interested {
                                                    ctx.accent_teal()
                                                } else {
                                                    ctx.theme.semantic.surface1
                                                },
                                            )),
                                        ),
                                        Span::styled(
                                            "■",
                                            ctx.apply(Style::default().fg(if peer.am_choking {
                                                ctx.accent_peach()
                                            } else {
                                                ctx.theme.semantic.surface1
                                            })),
                                        ),
                                    ])
                                    .into(),
                                    PeerColumnId::Address => {
                                        let display = if app_state.anonymize_torrent_names {
                                            "xxx.xxx.xxx"
                                        } else {
                                            &peer.address
                                        };
                                        Cell::from(display.to_string())
                                    }
                                    PeerColumnId::Client => {
                                        let raw_client = parse_peer_id(&peer.peer_id);
                                        Cell::from(sanitize_text(&raw_client))
                                    }
                                    PeerColumnId::Action => Cell::from(peer.last_action.clone()),
                                    PeerColumnId::Progress => {
                                        let total = state.number_of_pieces_total as usize;
                                        let pct = if total > 0 {
                                            let c = peer
                                                .bitfield
                                                .iter()
                                                .take(total)
                                                .filter(|&&b| b)
                                                .count();
                                            (c as f64 / total as f64) * 100.0
                                        } else {
                                            0.0
                                        };
                                        Cell::from(format!("{:.0}%", pct))
                                    }
                                    PeerColumnId::DownSpeed => {
                                        if peers_chunk.width > 120 {
                                            Cell::from(format!(
                                                "{} ({})",
                                                format_speed(peer.download_speed_bps),
                                                format_bytes(peer.total_downloaded)
                                            ))
                                        } else {
                                            Cell::from(format_speed(peer.download_speed_bps))
                                        }
                                    }
                                    PeerColumnId::UpSpeed => {
                                        if peers_chunk.width > 120 {
                                            Cell::from(format!(
                                                "{} ({})",
                                                format_speed(peer.upload_speed_bps),
                                                format_bytes(peer.total_uploaded)
                                            ))
                                        } else {
                                            Cell::from(format_speed(peer.upload_speed_bps))
                                        }
                                    }
                                }
                            })
                            .collect();
                        Row::new(cells).style(ctx.apply(Style::default().fg(row_color)))
                    });

                    let peers_table = Table::new(peer_rows, constraints)
                        .header(peer_header)
                        .block(Block::default());

                    let table_rows_needed: u16 = 1 + peers_to_display.len() as u16;
                    let peer_block_height_needed: u16 = table_rows_needed + 1;
                    let remaining_height =
                        peers_chunk.height.saturating_sub(peer_block_height_needed);
                    const MIN_HEATMAP_HEIGHT: u16 = 4;

                    let peers_block = Block::default()
                        .padding(Padding::new(1, 1, 0, 0))
                        .border_style(peer_border_style);

                    if remaining_height >= MIN_HEATMAP_HEIGHT {
                        let layout_chunks = Layout::vertical([
                            Constraint::Length(peer_block_height_needed),
                            Constraint::Min(0),
                        ])
                        .split(peers_chunk);
                        let inner_peers_area = peers_block.inner(layout_chunks[0]);
                        f.render_widget(peers_block, layout_chunks[0]);
                        f.render_widget(peers_table, inner_peers_area);
                        draw_swarm_heatmap(
                            f,
                            ctx,
                            &state.peers,
                            state.number_of_pieces_total,
                            layout_chunks[1],
                        );
                    } else {
                        let inner_peers_area = peers_block.inner(peers_chunk);
                        f.render_widget(peers_block, peers_chunk);
                        f.render_widget(peers_table, inner_peers_area);
                    }
                }
            }
        }
    } else {
        draw_swarm_heatmap(f, ctx, &[], 0, peers_chunk);
    }
}

fn draw_swarm_heatmap(
    f: &mut Frame,
    ctx: &ThemeContext,
    peers: &[PeerInfo],
    total_pieces: u32,
    area: Rect,
) {
    let color_status_low = ctx.apply(
        Style::default()
            .fg(ctx.state_error())
            .add_modifier(Modifier::DIM),
    );
    let color_status_medium = ctx.apply(
        Style::default()
            .fg(ctx.state_warning())
            .add_modifier(Modifier::DIM),
    );
    let color_status_high = ctx.apply(
        Style::default()
            .fg(ctx.state_info())
            .add_modifier(Modifier::DIM),
    );
    let color_status_complete = ctx.apply(
        Style::default()
            .fg(ctx.state_complete())
            .add_modifier(Modifier::BOLD),
    );
    let color_status_empty = ctx.apply(Style::default().fg(ctx.theme.semantic.subtext1));
    let color_status_waiting = ctx.apply(Style::default().fg(ctx.theme.semantic.subtext1));

    let color_heatmap_low = ctx.theme.scale.heatmap.low;
    let color_heatmap_medium = ctx.theme.scale.heatmap.medium;
    let color_heatmap_high = ctx.theme.scale.heatmap.high;
    let color_heatmap_empty = ctx.theme.scale.heatmap.empty;

    let shade_light = symbols::shade::LIGHT;
    let shade_medium = symbols::shade::MEDIUM;
    let shade_dark = symbols::shade::DARK;

    let total_pieces_usize = total_pieces as usize;
    let mut availability: Vec<u32> = vec![0; total_pieces_usize];
    if total_pieces_usize > 0 {
        for peer in peers {
            for (i, has_piece) in peer.bitfield.iter().enumerate().take(total_pieces_usize) {
                if *has_piece {
                    availability[i] += 1;
                }
            }
        }
    }

    let max_avail = availability.iter().max().copied().unwrap_or(0);
    let pieces_available_in_swarm = availability.iter().filter(|&&count| count > 0).count();
    let is_swarm_complete =
        total_pieces_usize > 0 && pieces_available_in_swarm == total_pieces_usize;
    let total_peers = peers.len();

    let (status_text, status_style) = if total_pieces_usize == 0 {
        ("Waiting...".to_string(), color_status_waiting)
    } else if is_swarm_complete {
        ("Complete".to_string(), color_status_complete)
    } else if max_avail == 0 {
        ("Empty".to_string(), color_status_empty)
    } else if total_peers == 0 {
        ("Low (0%)".to_string(), color_status_low)
    } else {
        let availability_percentage =
            (pieces_available_in_swarm as f64 / total_pieces_usize as f64) * 100.0;
        if availability_percentage < 33.3 {
            (
                format!("Low ({:.0}%)", availability_percentage),
                color_status_low,
            )
        } else if availability_percentage < 66.6 {
            (
                format!("Medium ({:.0}%)", availability_percentage),
                color_status_medium,
            )
        } else {
            (
                format!("High ({:.0}%)", availability_percentage),
                color_status_high,
            )
        }
    };

    let title = Line::from(vec![
        Span::styled(
            " Swarm Availability: ",
            ctx.apply(Style::default().fg(ctx.state_complete())),
        ),
        Span::styled(status_text, status_style),
    ]);
    let block = Block::default()
        .title(title)
        .borders(Borders::NONE)
        .padding(Padding::new(1, 1, 0, 1))
        .border_style(ctx.apply(Style::default().fg(ctx.theme.semantic.border)));
    let inner_area = block.inner(area);
    f.render_widget(block, area);

    if total_pieces_usize == 0 {
        let available_width = inner_area.width as usize;
        let available_height = inner_area.height as usize;
        let mut lines = Vec::with_capacity(available_height);

        for _ in 0..available_height {
            let row_str = shade_light.repeat(available_width);
            lines.push(Line::from(Span::styled(
                row_str,
                ctx.apply(Style::default().fg(ctx.theme.semantic.surface1)),
            )));
        }

        let heatmap = Paragraph::new(lines);
        f.render_widget(heatmap, inner_area);
        return;
    }

    let max_avail_f64 = max_avail.max(5) as f64;
    let available_width = inner_area.width as usize;
    let available_height = inner_area.height as usize;
    let total_cells = (available_width * available_height) as u64;

    if total_cells == 0 {
        return;
    }

    let mut lines = Vec::with_capacity(available_height);
    let total_pieces_u64 = total_pieces_usize as u64;

    for y in 0..available_height {
        let mut spans = Vec::with_capacity(available_width);
        for x in 0..available_width {
            let cell_index = (y * available_width + x) as u64;
            let piece_index = ((cell_index * total_pieces_u64) / total_cells) as usize;
            if piece_index >= total_pieces_usize {
                spans.push(Span::raw(" "));
                continue;
            }
            let count = availability[piece_index];
            let (piece_char, color) = if count == 0 {
                (shade_light, color_heatmap_empty)
            } else {
                let norm_val = count as f64 / max_avail_f64;
                if norm_val < 0.20 {
                    (shade_light, color_heatmap_low)
                } else if norm_val < 0.80 {
                    (shade_medium, color_heatmap_medium)
                } else {
                    (shade_dark, color_heatmap_high)
                }
            };
            spans.push(Span::styled(
                piece_char.to_string(),
                ctx.apply(Style::default().fg(color)),
            ));
        }
        lines.push(Line::from(spans));
    }
    let heatmap = Paragraph::new(lines);
    f.render_widget(heatmap, inner_area);
}

pub async fn handle_event(event: CrosstermEvent, app: &mut App) {
    match event {
        CrosstermEvent::Key(key) => {
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Esc => {
                        app.app_state.system_error = None;
                    }
                    KeyCode::Char('/') => {
                        app.app_state.is_searching = true;
                        app.app_state.selected_torrent_index = 0;
                    }
                    KeyCode::Char('x') => {
                        app.app_state.anonymize_torrent_names = !app.app_state.anonymize_torrent_names;
                    }
                    KeyCode::Char('z') => {
                        app.app_state.mode = AppMode::PowerSaving;
                        return;
                    }
                    KeyCode::Char('Q') => {
                        app.app_state.should_quit = true;
                    }
                    KeyCode::Char('c') => {
                        let items = ConfigItem::iter().collect::<Vec<_>>();
                        app.app_state.mode = AppMode::Config {
                            settings_edit: Box::new(app.client_configs.clone()),
                            selected_index: 0,
                            items,
                            editing: None,
                        };
                    }
                    KeyCode::Char('t') => {
                        app.app_state.graph_mode = app.app_state.graph_mode.next();
                    }
                    KeyCode::Char('T') => {
                        app.app_state.graph_mode = app.app_state.graph_mode.prev();
                    }
                    KeyCode::Char('[') | KeyCode::Char('{') => {
                        app.app_state.data_rate = app.app_state.data_rate.next_slower();
                        let new_rate = app.app_state.data_rate.as_ms();

                        for manager_tx in app.torrent_manager_command_txs.values() {
                            let _ = manager_tx.try_send(ManagerCommand::SetDataRate(new_rate));
                        }
                    }
                    KeyCode::Char(']') | KeyCode::Char('}') => {
                        app.app_state.data_rate = app.app_state.data_rate.next_faster();
                        let new_rate = app.app_state.data_rate.as_ms();

                        for manager_tx in app.torrent_manager_command_txs.values() {
                            let _ = manager_tx.try_send(ManagerCommand::SetDataRate(new_rate));
                        }
                    }
                    KeyCode::Char('<') => {
                        let themes = crate::theme::ThemeName::sorted_for_ui();
                        let current_idx = themes
                            .iter()
                            .position(|&t| t == app.client_configs.ui_theme)
                            .unwrap_or(0);
                        let new_idx = if current_idx == 0 {
                            themes.len() - 1
                        } else {
                            current_idx - 1
                        };
                        app.client_configs.ui_theme = themes[new_idx];
                        app.app_state.theme = crate::theme::Theme::builtin(themes[new_idx]);
                        let _ = app
                            .app_command_tx
                            .try_send(AppCommand::UpdateConfig(app.client_configs.clone()));
                    }
                    KeyCode::Char('>') => {
                        let themes = crate::theme::ThemeName::sorted_for_ui();
                        let current_idx = themes
                            .iter()
                            .position(|&t| t == app.client_configs.ui_theme)
                            .unwrap_or(0);
                        let new_idx = (current_idx + 1) % themes.len();
                        app.client_configs.ui_theme = themes[new_idx];
                        app.app_state.theme = crate::theme::Theme::builtin(themes[new_idx]);
                        let _ = app
                            .app_command_tx
                            .try_send(AppCommand::UpdateConfig(app.client_configs.clone()));
                    }
                    KeyCode::Char('p') => {
                        if let Some(info_hash) = app
                            .app_state
                            .torrent_list_order
                            .get(app.app_state.selected_torrent_index)
                        {
                            if let (Some(torrent_display), Some(torrent_manager_command_tx)) = (
                                app.app_state.torrents.get_mut(info_hash),
                                app.torrent_manager_command_txs.get(info_hash),
                            ) {
                                let (new_state, command) =
                                    match torrent_display.latest_state.torrent_control_state {
                                        TorrentControlState::Running => (
                                            TorrentControlState::Paused,
                                            crate::torrent_manager::ManagerCommand::Pause,
                                        ),
                                        TorrentControlState::Paused => (
                                            TorrentControlState::Running,
                                            crate::torrent_manager::ManagerCommand::Resume,
                                        ),
                                        TorrentControlState::Deleting => return,
                                    };
                                torrent_display.latest_state.torrent_control_state = new_state;
                                let torrent_manager_command_tx_clone = torrent_manager_command_tx.clone();
                                tokio::spawn(async move {
                                    let _ = torrent_manager_command_tx_clone.send(command).await;
                                });
                            }
                        }
                    }
                    KeyCode::Char('a') => {
                        let initial_path = app.get_initial_source_path();
                        let _ = app.app_command_tx.try_send(AppCommand::FetchFileTree {
                            path: initial_path,
                            browser_mode: FileBrowserMode::File(vec![".torrent".to_string()]),
                            highlight_path: None,
                        });
                    }
                    KeyCode::Char('d') => {
                        if let Some(info_hash) = app
                            .app_state
                            .torrent_list_order
                            .get(app.app_state.selected_torrent_index)
                            .cloned()
                        {
                            app.app_state.mode = AppMode::DeleteConfirm {
                                info_hash,
                                with_files: false,
                            };
                        }
                    }
                    KeyCode::Char('D') => {
                        if let Some(info_hash) = app
                            .app_state
                            .torrent_list_order
                            .get(app.app_state.selected_torrent_index)
                            .cloned()
                        {
                            app.app_state.mode = AppMode::DeleteConfirm {
                                info_hash,
                                with_files: true,
                            };
                        }
                    }
                    KeyCode::Char('s') => {
                        let layout_ctx = LayoutContext::new(app.app_state.screen_area, &app.app_state, 35);
                        let layout_plan = calculate_layout(app.app_state.screen_area, &layout_ctx);
                        let (_, visible_torrent_columns) =
                            compute_visible_torrent_columns(&app.app_state, layout_plan.list.width);
                        let (_, visible_peer_columns) =
                            compute_visible_peer_columns(layout_plan.peers.width);
                        match app.app_state.selected_header {
                            SelectedHeader::Torrent(i) => {
                                let cols = get_torrent_columns();

                                if let Some(def) =
                                    visible_torrent_columns.get(i).and_then(|&real_idx| cols.get(real_idx))
                                {
                                    if let Some(column) = def.sort_enum {
                                        if app.app_state.torrent_sort.0 == column {
                                            app.app_state.torrent_sort.1 =
                                                if app.app_state.torrent_sort.1 == SortDirection::Ascending {
                                                    SortDirection::Descending
                                                } else {
                                                    SortDirection::Ascending
                                                };
                                        } else {
                                            app.app_state.torrent_sort.0 = column;
                                            app.app_state.torrent_sort.1 = SortDirection::Descending;
                                        }
                                        app.sort_and_filter_torrent_list();
                                    }
                                }
                            }
                            SelectedHeader::Peer(i) => {
                                let cols = get_peer_columns();

                                if let Some(def) =
                                    visible_peer_columns.get(i).and_then(|&real_idx| cols.get(real_idx))
                                {
                                    if let Some(column) = def.sort_enum {
                                        if app.app_state.peer_sort.0 == column {
                                            app.app_state.peer_sort.1 =
                                                if app.app_state.peer_sort.1 == SortDirection::Ascending {
                                                    SortDirection::Descending
                                                } else {
                                                    SortDirection::Ascending
                                                };
                                        } else {
                                            app.app_state.peer_sort.0 = column;
                                            app.app_state.peer_sort.1 = SortDirection::Descending;
                                        }
                                    }
                                }
                            }
                        };
                    }
                    KeyCode::Up
                    | KeyCode::Char('k')
                    | KeyCode::Down
                    | KeyCode::Char('j')
                    | KeyCode::Left
                    | KeyCode::Char('h')
                    | KeyCode::Right
                    | KeyCode::Char('l') => {
                        handle_navigation(&mut app.app_state, key.code);
                    }
                    #[cfg(windows)]
                    KeyCode::Char('v') => match ClipboardContext::new() {
                        Ok(mut ctx) => match ctx.get_contents() {
                            Ok(text) => {
                                handle_pasted_text(app, text.trim()).await;
                            }
                            Err(e) => {
                                tracing_event!(Level::ERROR, "Clipboard read error: {}", e);
                                app.app_state.system_error = Some(format!("Clipboard read error: {}", e));
                            }
                        },
                        Err(e) => {
                            tracing_event!(Level::ERROR, "Clipboard context error: {}", e);
                            app.app_state.system_error =
                                Some(format!("Clipboard initialization error: {}", e));
                        }
                    },
                    _ => {}
                }
            }
        }
        #[cfg(not(windows))]
        CrosstermEvent::Paste(pasted_text) => {
            handle_pasted_text(app, pasted_text.trim()).await;
        }
        _ => {}
    }
}

// SPDX-FileCopyrightText: 2025 The superseedr Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::app::{App, AppMode, TorrentControlState};
use crate::tui::screen_context::ScreenContext;
use crate::torrent_manager::ManagerCommand;
use crate::tui::formatters::{centered_rect, sanitize_text};
use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::prelude::{Frame, Line, Span, Style, Stylize};
use ratatui::widgets::{Block, Borders, Clear, Padding, Paragraph, Wrap};
use ratatui::crossterm::event::{Event as CrosstermEvent, KeyCode};

pub fn draw(f: &mut Frame, screen: &ScreenContext<'_>) {
    let app_state = screen.ui;
    let ctx = screen.theme;

    if !matches!(app_state.mode, AppMode::DeleteConfirm) {
        return;
    }

    let info_hash = &app_state.ui.delete_confirm.info_hash;
    let with_files = app_state.ui.delete_confirm.with_files;

    if let Some(torrent_to_delete) = app_state.torrents.get(info_hash) {
        let terminal_area = f.area();
        let rect_width = if terminal_area.width < 60 { 90 } else { 50 };
        let rect_height = if terminal_area.height < 20 { 95 } else { 18 };

        let area = centered_rect(rect_width, rect_height, terminal_area);
        f.render_widget(Clear, area);

        let vert_padding = if area.height < 10 { 0 } else { 1 };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(ctx.apply(Style::default().fg(ctx.state_error())))
            .padding(Padding::new(2, 2, vert_padding, vert_padding));

        let inner_area = block.inner(area);
        f.render_widget(block, area);

        let chunks = Layout::vertical([
            Constraint::Length(2),
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner_area);

        let name = sanitize_text(&torrent_to_delete.latest_state.torrent_name);
        let path = torrent_to_delete
            .latest_state
            .download_path
            .as_ref()
            .map(|p| sanitize_text(&p.to_string_lossy()))
            .unwrap_or_else(|| "Unknown Path".to_string());

        f.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    name,
                    ctx.apply(Style::default().fg(ctx.state_warning()).bold().underlined()),
                )),
                Line::from(Span::styled(
                    path,
                    ctx.apply(Style::default().fg(ctx.theme.semantic.text)),
                )),
            ])
            .alignment(Alignment::Center),
            chunks[0],
        );

        if chunks[1].height > 0 {
            let body = if with_files {
                vec![
                    Line::from(""),
                    Line::from(Span::styled(
                        "⚠️ PERMANENT TORRENT FILES DELETION ON ⚠️",
                        ctx.apply(Style::default().fg(ctx.state_error()).bold()),
                    )),
                    Line::from(vec![
                        Span::raw("All local data for this torrent will be "),
                        Span::styled(
                            "ERASED",
                            ctx.apply(Style::default().fg(ctx.state_error()).bold().underlined()),
                        ),
                    ]),
                ]
            } else {
                vec![
                    Line::from(""),
                    Line::from(Span::styled(
                        "Safe Removal (Files Kept)",
                        ctx.apply(Style::default().fg(ctx.state_success())),
                    )),
                    Line::from(vec![
                        Span::raw("Use "),
                        Span::styled(
                            "[D]",
                            ctx.apply(Style::default().fg(ctx.state_warning()).bold()),
                        ),
                        Span::raw(" to remove files..."),
                    ]),
                ]
            };
            f.render_widget(
                Paragraph::new(body)
                    .alignment(Alignment::Center)
                    .wrap(Wrap { trim: true }),
                chunks[1],
            );
        }

        let actions = Line::from(vec![
            Span::styled(
                "[Enter]",
                ctx.apply(Style::default().fg(ctx.state_success()).bold()),
            ),
            Span::raw(" Confirm  "),
            Span::styled("[Esc]", ctx.apply(Style::default().fg(ctx.state_error()))),
            Span::raw(" Cancel"),
        ]);

        f.render_widget(
            Paragraph::new(actions).alignment(Alignment::Center),
            chunks[3],
        );
    }
}

pub fn handle_event(event: CrosstermEvent, app: &mut App) -> bool {
    if let CrosstermEvent::Key(key) = event {
        let info_hash = app.app_state.ui.delete_confirm.info_hash.clone();
        let with_files = app.app_state.ui.delete_confirm.with_files;
        match key.code {
            KeyCode::Enter => {
                let command = if with_files {
                    ManagerCommand::DeleteFile
                } else {
                    ManagerCommand::Shutdown
                };
                if let Some(manager_tx) = app.torrent_manager_command_txs.get(&info_hash) {
                    let manager_tx_clone = manager_tx.clone();
                    tokio::spawn(async move {
                        let _ = manager_tx_clone.send(command).await;
                    });
                }
                if let Some(torrent) = app.app_state.torrents.get_mut(&info_hash) {
                    torrent.latest_state.torrent_control_state = TorrentControlState::Deleting;
                }
                return true;
            }
            KeyCode::Esc => return true,
            _ => {}
        }
    }

    false
}

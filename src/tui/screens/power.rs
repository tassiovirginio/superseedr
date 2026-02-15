// SPDX-FileCopyrightText: 2025 The superseedr Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::app::{AppMode, AppState};
use ratatui::crossterm::event::{Event as CrosstermEvent, KeyCode, KeyEventKind};

pub fn handle_event(event: CrosstermEvent, app_state: &mut AppState) {
    if let CrosstermEvent::Key(key) = event {
        if key.kind == KeyEventKind::Press {
            if let KeyCode::Char('z') = key.code {
                app_state.mode = AppMode::Normal;
            }
        }
    }
}

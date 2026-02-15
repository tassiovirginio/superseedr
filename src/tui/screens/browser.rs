// SPDX-FileCopyrightText: 2025 The superseedr Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::app::FileBrowserMode;
use ratatui::crossterm::event::KeyCode;

pub fn handle_search_interceptor(
    key_code: KeyCode,
    is_searching: &mut bool,
    search_query: &mut String,
) -> bool {
    if !*is_searching {
        return false;
    }

    match key_code {
        KeyCode::Esc => {
            *is_searching = false;
            search_query.clear();
        }
        KeyCode::Enter => {
            *is_searching = false;
        }
        KeyCode::Backspace => {
            search_query.pop();
        }
        KeyCode::Char(c) => {
            search_query.push(c);
        }
        _ => {}
    }

    true
}

pub fn handle_download_name_edit_guard(key_code: KeyCode, browser_mode: &mut FileBrowserMode) -> bool {
    if let FileBrowserMode::DownloadLocSelection {
        container_name,
        is_editing_name,
        cursor_pos,
        original_name_backup,
        ..
    } = browser_mode
    {
        if !*is_editing_name {
            return false;
        }

        match key_code {
            KeyCode::Enter => {
                *is_editing_name = false;
            }
            KeyCode::Esc => {
                *container_name = original_name_backup.clone();
                *is_editing_name = false;
                *cursor_pos = container_name.len();
            }
            KeyCode::Left => {
                *cursor_pos = cursor_pos.saturating_sub(1);
            }
            KeyCode::Right => {
                if *cursor_pos < container_name.len() {
                    *cursor_pos += 1;
                }
            }
            KeyCode::Backspace => {
                if *cursor_pos > 0 {
                    container_name.remove(*cursor_pos - 1);
                    *cursor_pos -= 1;
                }
            }
            KeyCode::Delete => {
                if *cursor_pos < container_name.len() {
                    container_name.remove(*cursor_pos);
                }
            }
            KeyCode::Char(c) => {
                container_name.insert(*cursor_pos, c);
                *cursor_pos += 1;
            }
            _ => {}
        }

        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{BrowserPane, ConfigItem, TorrentPreviewPayload};
    use crate::tui::tree::{RawNode, TreeViewState};
    use std::path::PathBuf;

    #[test]
    fn search_interceptor_clears_on_escape() {
        let mut is_searching = true;
        let mut query = String::from("abc");
        let consumed = handle_search_interceptor(KeyCode::Esc, &mut is_searching, &mut query);
        assert!(consumed);
        assert!(!is_searching);
        assert!(query.is_empty());
    }

    #[test]
    fn name_edit_guard_updates_buffer_and_cursor() {
        let mut mode = FileBrowserMode::DownloadLocSelection {
            torrent_files: vec![],
            container_name: "ab".to_string(),
            use_container: true,
            is_editing_name: true,
            focused_pane: BrowserPane::TorrentPreview,
            preview_tree: vec![RawNode {
                name: "x".to_string(),
                full_path: PathBuf::from("x"),
                children: vec![],
                payload: TorrentPreviewPayload::default(),
                is_dir: false,
            }],
            preview_state: TreeViewState::default(),
            cursor_pos: 2,
            original_name_backup: "ab".to_string(),
        };

        let consumed = handle_download_name_edit_guard(KeyCode::Char('c'), &mut mode);
        assert!(consumed);
        match mode {
            FileBrowserMode::DownloadLocSelection {
                container_name,
                cursor_pos,
                ..
            } => {
                assert_eq!(container_name, "abc");
                assert_eq!(cursor_pos, 3);
            }
            _ => panic!("expected DownloadLocSelection"),
        }
    }

    #[test]
    fn name_edit_guard_ignored_when_not_editing() {
        let mut mode = FileBrowserMode::ConfigPathSelection {
            target_item: ConfigItem::WatchFolder,
            current_settings: Box::default(),
            selected_index: 0,
            items: vec![],
        };
        let consumed = handle_download_name_edit_guard(KeyCode::Char('x'), &mut mode);
        assert!(!consumed);
    }
}

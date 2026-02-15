// SPDX-FileCopyrightText: 2025 The superseedr Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::app::{BrowserPane, FileBrowserMode, FileMetadata};
use crate::tui::formatters::centered_rect;
use crate::tui::layout::calculate_file_browser_layout;
use crate::tui::tree::TreeFilter;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;

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

pub fn handle_download_shortcuts(key_code: KeyCode, browser_mode: &mut FileBrowserMode) -> bool {
    if let FileBrowserMode::DownloadLocSelection {
        container_name,
        use_container,
        is_editing_name,
        focused_pane,
        cursor_pos,
        original_name_backup,
        ..
    } = browser_mode
    {
        match key_code {
            KeyCode::Char('x') => {
                *use_container = !*use_container;
                true
            }
            KeyCode::Char('r') if *use_container => {
                *is_editing_name = true;
                *original_name_backup = container_name.clone();
                *cursor_pos = container_name.len();
                *focused_pane = BrowserPane::TorrentPreview;
                true
            }
            KeyCode::Tab => {
                *focused_pane = match focused_pane {
                    BrowserPane::FileSystem => BrowserPane::TorrentPreview,
                    BrowserPane::TorrentPreview => BrowserPane::FileSystem,
                };
                true
            }
            _ => false,
        }
    } else {
        false
    }
}

pub fn has_preview_content(
    browser_mode: &FileBrowserMode,
    pending_torrent_path: bool,
    pending_torrent_link: bool,
    cursor_path: Option<&std::path::PathBuf>,
) -> bool {
    match browser_mode {
        FileBrowserMode::DownloadLocSelection { .. } => pending_torrent_path || pending_torrent_link,
        FileBrowserMode::File(_) => {
            cursor_path.is_some_and(|p| p.extension().is_some_and(|ext| ext == "torrent"))
        }
        _ => false,
    }
}

pub fn focused_pane(browser_mode: &FileBrowserMode) -> BrowserPane {
    if let FileBrowserMode::DownloadLocSelection { focused_pane, .. } = browser_mode {
        focused_pane.clone()
    } else {
        BrowserPane::FileSystem
    }
}

pub fn calculate_area(screen: Rect, has_preview_content: bool) -> Rect {
    if has_preview_content {
        if screen.width < 60 {
            screen
        } else {
            centered_rect(90, 80, screen)
        }
    } else if screen.width < 40 {
        screen
    } else {
        centered_rect(75, 80, screen)
    }
}

pub fn calculate_list_height(
    screen: Rect,
    has_preview_content: bool,
    is_searching: bool,
    focused_pane: &BrowserPane,
) -> usize {
    let area = calculate_area(screen, has_preview_content);
    let layout = calculate_file_browser_layout(area, has_preview_content, is_searching, focused_pane);
    layout.list.height.saturating_sub(2) as usize
}

pub fn build_filter(browser_mode: &FileBrowserMode, search_query: &str) -> TreeFilter<FileMetadata> {
    match browser_mode {
        FileBrowserMode::Directory
        | FileBrowserMode::DownloadLocSelection { .. }
        | FileBrowserMode::ConfigPathSelection { .. } => TreeFilter::from_text(search_query),
        FileBrowserMode::File(extensions) => {
            let exts = extensions.clone();
            TreeFilter::new(search_query, move |node| {
                node.is_dir || exts.iter().any(|ext| node.name.ends_with(ext))
            })
        }
    }
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

    #[test]
    fn download_shortcuts_toggle_pane() {
        let mut mode = FileBrowserMode::DownloadLocSelection {
            torrent_files: vec![],
            container_name: "x".to_string(),
            use_container: true,
            is_editing_name: false,
            focused_pane: BrowserPane::FileSystem,
            preview_tree: vec![],
            preview_state: TreeViewState::default(),
            cursor_pos: 1,
            original_name_backup: "x".to_string(),
        };
        let consumed = handle_download_shortcuts(KeyCode::Tab, &mut mode);
        assert!(consumed);
        match mode {
            FileBrowserMode::DownloadLocSelection { focused_pane, .. } => {
                assert_eq!(focused_pane, BrowserPane::TorrentPreview);
            }
            _ => panic!("expected DownloadLocSelection"),
        }
    }

    #[test]
    fn has_preview_content_matches_file_mode_torrent_extension() {
        let mode = FileBrowserMode::File(vec![".torrent".to_string()]);
        let path = PathBuf::from("demo.torrent");
        assert!(has_preview_content(&mode, false, false, Some(&path)));
    }
}

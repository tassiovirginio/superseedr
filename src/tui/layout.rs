// SPDX-FileCopyrightText: 2025 The superseedr Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

pub mod browser;
pub mod common;
pub mod normal;

pub use common::{
    ColumnId, PeerColumnId, SmartCol, compute_smart_table_layout, compute_visible_peer_columns,
    compute_visible_torrent_columns, get_peer_columns, get_torrent_columns,
};
pub use normal::{LayoutContext, LayoutPlan, MIN_SIDEBAR_WIDTH, calculate_layout};

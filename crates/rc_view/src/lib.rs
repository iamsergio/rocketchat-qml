// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What the screens show for each account: the caches, the views, and the row operations that turn
//! core events into list changes. Plain Rust that knows no GUI toolkit, so the Qt frontend and the
//! coming Slint one both use it; rendering rich text is each frontend's job.

#![forbid(unsafe_code)]

pub mod accounts;
pub mod attachment_view;
pub mod avatar_view;
pub mod cache;
pub mod dev_login;
pub mod directory;
pub mod emoji_image_view;
pub mod emoji_picker_view;
pub mod file_urls;
pub mod message_list;
pub mod message_view;
pub mod paste;
pub mod quote_view;
pub mod reaction_view;
pub mod rich_html;
pub mod rich_view;
pub mod room_search;
pub mod room_view;
pub mod row_ops;
pub mod session_view;
pub mod staged_view;
pub mod startup;
pub mod switchboard;
pub mod thread_list_view;
pub mod thread_view;
pub mod upload_view;
pub mod viewer;

// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Domain types, session state, the room list, the message timeline and use cases. Knows nothing about Qt or Slint.

#![forbid(unsafe_code)]

mod account;
pub mod adapters;
mod avatars;
mod backoff;
mod client;
mod core;
pub mod domain;
mod downloads;
pub mod emoji;
mod emoji_images;
mod error;
mod images;
pub mod list_change;
pub mod logging;
pub mod policy;
pub mod ports;
mod quote;
mod random;
pub mod rich;
pub mod rooms;
pub mod session;
mod store_worker;
pub mod threads;
pub mod timeline;
mod tuning;
mod typing;
mod users;

pub use avatars::AvatarUpdate;
pub use client::{AccountHandle, Client, Connection, Event};
pub use emoji_images::EmojiImageUpdate;
pub use error::Error;
pub use images::ImageUpdate;

/// Application version, from `version.txt` at the repository root.
pub const VERSION: &str = include_str!("../../../version.txt").trim_ascii();

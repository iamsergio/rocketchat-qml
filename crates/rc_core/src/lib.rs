// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Domain types, session state, the room list, the message timeline and use cases. Knows nothing about Qt or Slint.

#![forbid(unsafe_code)]

pub mod adapters;
mod backoff;
mod client;
pub mod domain;
mod error;
pub mod logging;
pub mod ports;
pub mod rooms;
pub mod session;
pub mod timeline;

pub use client::{Client, Event};
pub use error::Error;

/// Application version, from `version.txt` at the repository root.
pub const VERSION: &str = include_str!("../../../version.txt").trim_ascii();

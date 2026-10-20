// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Typed client for the Rocket.Chat REST API and the realtime DDP protocol. Keeps no application state.
//!
//! The REST side starts at [`RestClient`]: check the server with [`RestClient::info`], log in, then
//! use the [`RestSession`] for authenticated calls. The serde wire types are private; callers only
//! see the plain types in this crate.

#![forbid(unsafe_code)]

mod client;
mod error;
mod types;
mod wire;

pub use client::{RestClient, RestSession};
pub use error::Error;
pub use types::{
    Author, Credentials, HistoryPage, LoginResult, Message, RoomType, ServerInfo, ServerVersion,
    Subscription, User,
};

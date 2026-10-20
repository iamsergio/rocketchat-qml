// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Typed client for the Rocket.Chat REST API and the realtime DDP protocol. Keeps no application state.
//!
//! The REST side starts at [`RestClient`]: check the server with [`RestClient::info`], log in, then
//! use the [`RestSession`] for authenticated calls. The realtime side is a [`DdpClient`]: connect,
//! log in with the credentials the REST login gave, subscribe, and read the [`DdpEvent`]s. The
//! serde wire types are private; callers only see the plain types in this crate.

#![forbid(unsafe_code)]

mod client;
mod ddp;
mod error;
pub mod md;
mod types;
mod wire;

pub use client::{RestClient, RestSession};
pub use ddp::{DdpClient, DdpEvents};
pub use error::Error;
pub use types::{
    Attachment, AttachmentImage, Author, AvatarOwner, Credentials, CustomEmoji, DdpEvent, Download,
    HistoryPage, LoginResult, Message, RoomType, ServerInfo, ServerVersion, Subscription, User,
};

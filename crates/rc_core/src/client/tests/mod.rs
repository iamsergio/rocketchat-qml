// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The session actor driven through a fake server API and in-memory stores. Plain `#[test]`s: the
//! client owns its runtime and can't be dropped inside one. Nothing here touches the OS keyring.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use rc_api::ServerVersion;
use secrecy::{ExposeSecret, SecretString};
use tokio::sync::Notify;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::adapters::MemoryStore;
use crate::avatars::AvatarUpdate;
use crate::backoff::Backoff;
use crate::client::{AccountHandle, Client, Connection, Event};
use crate::domain::{
    Account, AccountId, Attachment, Author, AvatarOwner, Credentials, Delivery, Identity,
    ImageAttachment, Message, MessageId, MessageKind, Quote, Reaction, Room, RoomId, RoomKind,
    StoredAccount, SystemEvent, User, room_from,
};
use crate::emoji::CustomEmojis;
use crate::images::ImageUpdate;
use crate::policy::{ActionError, MessageAction, MessagePolicy};
use crate::ports::{
    ApiError, CacheError, CacheKey, Changes, Connector, FileCache, Image, Page, Pushed, Realtime,
    SecretStore, ServerApi, StoreError, Stored, ThreadTarget, image_extension,
};
use crate::rooms::{RoomAction, RoomActionError, RoomChange, Unread};
use crate::session::{LoginFailure, SessionState};
use crate::timeline::{ThreadStatus, TimelineChange, TimelineError, TimelineStatus};

mod accounts;
mod activity;
mod avatars;
mod changes;
mod emoji;
mod fake;
mod harness;
mod images;
mod login;
mod mark_read;
mod message_actions;
mod quotes;
mod realtime;
mod room_actions;
mod rooms;
mod sending;
mod threads;
mod timeline;

use fake::*;
use harness::*;

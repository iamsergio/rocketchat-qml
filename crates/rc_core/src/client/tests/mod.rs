// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The session actor driven through a fake server API and in-memory stores. Plain `#[test]`s: the
//! client owns its runtime and can't be dropped inside one. Nothing here touches the OS keyring.

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
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
use crate::directory::{
    ActionState, Directory, DirectoryAction, DirectoryChannel, DirectoryError, DirectoryState,
    Found, Person,
};
use crate::domain::{
    Account, AccountId, Attachment, Author, AvatarOwner, Credentials, Delivery, Identity,
    ImageAttachment, Message, MessageId, MessageKind, Quote, Reaction, Room, RoomId, RoomKind,
    StoredAccount, SystemEvent, User, room_from,
};
use crate::emoji::CustomEmojis;
use crate::emoji_images::EmojiImageUpdate;
use crate::images::ImageUpdate;
use crate::members::{Member, MemberError, MemberSearch, MemberSearchState};
use crate::policy::{ActionError, MessageAction, MessagePolicy};
use crate::ports::{
    ApiError, CacheError, CacheKey, Changes, Clock, Connector, FileCache, Image, MessageWindow,
    NewerPage, Page, Pushed, Realtime, SecretStore, ServerApi, StoreError, Stored, ThreadTarget,
    ThreadsPage, image_extension,
};
use crate::rooms::{RoomAction, RoomActionError, RoomChange, Unread};
use crate::session::{LoginFailure, SessionState};
use crate::threads::{ThreadFilter, ThreadListStatus};
use crate::timeline::{ThreadStatus, TimelineChange, TimelineError, TimelineStatus};
use crate::tuning::Tuning;
use crate::uploads::{Upload, UploadError, UploadId, UploadState};

mod accounts;
mod activity;
mod around;
mod avatars;
mod batch;
mod changes;
mod directory;
mod emoji;
mod emoji_images;
mod fake;
mod harness;
mod images;
mod login;
mod mark_read;
mod members;
mod message_actions;
mod message_search;
mod quiet;
mod quotes;
mod realtime;
mod room_actions;
mod rooms;
mod sending;
mod thread_list;
mod threads;
mod timeline;
mod typing;
mod uploads;

use fake::*;
use harness::*;

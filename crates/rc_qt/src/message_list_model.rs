// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The `MessageList` model: the open room's messages, newest first (row 0 is the newest), for a
//! view that runs bottom to top. Older messages are inserted after the last row and newer ones at
//! row 0; only a change of room resets it.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++Qt" {
        include!(<QtCore/QAbstractListModel>);
        type QAbstractListModel = crate::room_list_model::qobject::QAbstractListModel;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qhash.h");
        type QHash_i32_QByteArray = cxx_qt_lib::QHash<cxx_qt_lib::QHashPair_i32_QByteArray>;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib/qmodelindex.h");
        type QModelIndex = cxx_qt_lib::QModelIndex;

        include!("cxx-qt-lib/qvector.h");
        type QVector_i32 = cxx_qt_lib::QVector<i32>;
    }

    unsafe extern "RustQt" {
        /// Available in QML as the `MessageList` singleton.
        #[qobject]
        #[qml_element = "MessageList"]
        #[qml_singleton]
        #[base = QAbstractListModel]
        type MessageListModel = super::MessageListModelRust;

        #[qsignal]
        #[inherit]
        #[cxx_name = "dataChanged"]
        fn data_changed(
            self: Pin<&mut MessageListModel>,
            top_left: &QModelIndex,
            bottom_right: &QModelIndex,
            roles: &QVector_i32,
        );

        #[inherit]
        fn index(
            self: &MessageListModel,
            row: i32,
            column: i32,
            parent: &QModelIndex,
        ) -> QModelIndex;
    }

    // The begin/end pairs must be kept balanced, so they are unsafe to call.
    extern "RustQt" {
        /// # Safety
        ///
        /// Call `end_insert_rows` after changing the data.
        #[inherit]
        #[cxx_name = "beginInsertRows"]
        unsafe fn begin_insert_rows(
            self: Pin<&mut MessageListModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        /// # Safety
        ///
        /// Only after `begin_insert_rows`.
        #[inherit]
        #[cxx_name = "endInsertRows"]
        unsafe fn end_insert_rows(self: Pin<&mut MessageListModel>);

        /// # Safety
        ///
        /// Call `end_remove_rows` after changing the data.
        #[inherit]
        #[cxx_name = "beginRemoveRows"]
        unsafe fn begin_remove_rows(
            self: Pin<&mut MessageListModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        /// # Safety
        ///
        /// Only after `begin_remove_rows`.
        #[inherit]
        #[cxx_name = "endRemoveRows"]
        unsafe fn end_remove_rows(self: Pin<&mut MessageListModel>);

        /// Returns `false` if the move is invalid, in which case there is nothing to end.
        ///
        /// # Safety
        ///
        /// If it returns `true`, call `end_move_rows` after changing the data.
        #[inherit]
        #[cxx_name = "beginMoveRows"]
        unsafe fn begin_move_rows(
            self: Pin<&mut MessageListModel>,
            source_parent: &QModelIndex,
            source_first: i32,
            source_last: i32,
            destination_parent: &QModelIndex,
            destination_child: i32,
        ) -> bool;
        /// # Safety
        ///
        /// Only after `begin_move_rows` returned `true`.
        #[inherit]
        #[cxx_name = "endMoveRows"]
        unsafe fn end_move_rows(self: Pin<&mut MessageListModel>);

        /// # Safety
        ///
        /// Call `end_reset_model` after changing the data.
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut MessageListModel>);
        /// # Safety
        ///
        /// Only after `begin_reset_model`.
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut MessageListModel>);
    }

    extern "RustQt" {
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &MessageListModel, parent: &QModelIndex) -> i32;

        #[cxx_override]
        fn data(self: &MessageListModel, index: &QModelIndex, role: i32) -> QVariant;

        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &MessageListModel) -> QHash_i32_QByteArray;
    }

    impl cxx_qt::Threading for MessageListModel {}
    impl cxx_qt::Initialize for MessageListModel {}
}

use core::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{
    QByteArray, QHash, QHashPair_i32_QByteArray, QList, QMap, QMapPair_QString_QVariant,
    QModelIndex, QString, QVariant, QVector,
};
use jiff::Timestamp;
use jiff::tz::TimeZone;
use rc_core::domain::{AvatarOwner, Message, MessageKind};
use rc_core::timeline::TimelineChange;

use crate::attachment_view::{AttachmentView, ImageUrls, attachments, paths, shows_image};
use crate::avatar_view::{AvatarUrls, color, initials, message_owner, owners, rows_using, runs};
use crate::hub;
use crate::message_view::{
    author_name, body, can_delete, can_edit, can_quote, can_react, day_label, delivery_name,
    first_of_day, html, mentions_me, system_text, time_text,
};
use crate::quote_view::{QuoteView, quotes};
use crate::reaction_view::{ReactionView, reactions};
use crate::rich_html::{Clock, Viewer};
use crate::row_ops::{MessageOp, apply_newest_first, message_ops};

use qobject::MessageListModel;

/// `Qt::DisplayRole`, which shows the body too.
const DISPLAY_ROLE: i32 = 0;

// The custom roles, from `Qt::UserRole` on (see `RoomListModel`).
const MESSAGE_ID_ROLE: i32 = 256;
const AUTHOR_ROLE: i32 = 257;
const USERNAME_ROLE: i32 = 258;
const TIME_ROLE: i32 = 259;
const BODY_ROLE: i32 = 260;
const IS_SYSTEM_ROLE: i32 = 261;
const SYSTEM_TEXT_ROLE: i32 = 262;
const FIRST_OF_DAY_ROLE: i32 = 263;
const DAY_LABEL_ROLE: i32 = 264;
const DELIVERY_ROLE: i32 = 265;
const HTML_ROLE: i32 = 266;
const MENTIONS_ME_ROLE: i32 = 267;
const AVATAR_ROLE: i32 = 268;
const AVATAR_INITIALS_ROLE: i32 = 269;
const AVATAR_COLOR_ROLE: i32 = 270;
const ATTACHMENTS_ROLE: i32 = 271;
const EDITED_ROLE: i32 = 272;
const REACTIONS_ROLE: i32 = 273;
const CAN_EDIT_ROLE: i32 = 274;
const CAN_DELETE_ROLE: i32 = 275;
const QUOTES_ROLE: i32 = 276;
const CAN_QUOTE_ROLE: i32 = 277;
const CAN_REACT_ROLE: i32 = 278;

/// Backing data of [`qobject::MessageListModel`]: the messages, mirrored from the core's changes
/// and held newest first.
pub struct MessageListModelRust {
    messages: Vec<Message>,
    /// Where times and days are read: the user's.
    zone: TimeZone,
    /// Who the messages are shown to: it decides the mentions that stand out.
    viewer: Viewer,
    avatars: AvatarUrls,
    images: ImageUrls,
}

impl Default for MessageListModelRust {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            zone: TimeZone::system(),
            viewer: Viewer::default(),
            avatars: AvatarUrls::default(),
            images: ImageUrls::default(),
        }
    }
}

/// `index` as the `int` Qt wants. A room never comes near `i32::MAX` messages.
fn row(index: usize) -> i32 {
    i32::try_from(index).unwrap_or(i32::MAX)
}

/// The attachments as a `QVariantList` of `QVariantMap`s, with the keys QML reads.
fn attachments_variant(views: &[AttachmentView]) -> QVariant {
    let text = |value: &str| QVariant::from(&QString::from(value));
    let mut list = QList::<QVariant>::default();
    for view in views {
        let mut map = QMap::<QMapPair_QString_QVariant>::default();
        for (key, value) in [
            ("title", text(&view.title)),
            ("description", text(&view.description)),
            ("size", text(&view.size)),
            ("link", text(&view.link)),
            ("isImage", QVariant::from(&view.is_image)),
            ("image", text(&view.image)),
            ("width", QVariant::from(&view.width)),
            ("height", QVariant::from(&view.height)),
        ] {
            map.insert(QString::from(key), value);
        }
        list.append(QVariant::from(&map));
    }
    QVariant::from(&list)
}

/// The reactions as a `QVariantList` of `QVariantMap`s, with the keys QML reads.
fn reactions_variant(views: &[ReactionView]) -> QVariant {
    let text = |value: &str| QVariant::from(&QString::from(value));
    let mut list = QList::<QVariant>::default();
    for view in views {
        let mut map = QMap::<QMapPair_QString_QVariant>::default();
        for (key, value) in [
            ("emoji", text(&view.emoji)),
            ("glyph", text(&view.glyph)),
            ("image", text(&view.image)),
            ("count", QVariant::from(&view.count)),
            ("mine", QVariant::from(&view.mine)),
            ("users", text(&view.users)),
        ] {
            map.insert(QString::from(key), value);
        }
        list.append(QVariant::from(&map));
    }
    QVariant::from(&list)
}

/// The quotes as a `QVariantList` of `QVariantMap`s, with the keys QML reads.
fn quotes_variant(views: &[QuoteView]) -> QVariant {
    let text = |value: &str| QVariant::from(&QString::from(value));
    let mut list = QList::<QVariant>::default();
    for view in views {
        let mut map = QMap::<QMapPair_QString_QVariant>::default();
        for (key, value) in [
            ("author", text(&view.author)),
            ("bodyHtml", text(&view.body_html)),
            ("body", text(&view.body)),
            ("time", text(&view.time)),
        ] {
            map.insert(QString::from(key), value);
        }
        list.append(QVariant::from(&map));
    }
    QVariant::from(&list)
}

impl cxx_qt::Initialize for MessageListModel {
    fn initialize(mut self: Pin<&mut Self>) {
        let (snapshot, viewer, avatars, images) = hub::register_messages(self.qt_thread());
        self.as_mut().rust_mut().viewer = viewer;
        self.as_mut().rust_mut().avatars = avatars;
        self.as_mut().rust_mut().images = images;
        self.as_mut().apply(&[snapshot]);
    }
}

impl MessageListModel {
    /// Applies the changes in order, announcing each one to the views.
    pub fn apply(mut self: Pin<&mut Self>, changes: &[TimelineChange]) {
        let no_parent = QModelIndex::default();
        for change in changes {
            for op in message_ops(change, self.messages.len()) {
                match op {
                    MessageOp::Reset => {
                        // SAFETY: balanced by `end_reset_model` below.
                        unsafe { self.as_mut().begin_reset_model() };
                        apply_newest_first(change, &mut self.as_mut().rust_mut().messages);
                        // SAFETY: after `begin_reset_model`.
                        unsafe { self.as_mut().end_reset_model() };
                    }
                    MessageOp::Insert { first, last } => {
                        // SAFETY: balanced by `end_insert_rows` below.
                        unsafe {
                            self.as_mut()
                                .begin_insert_rows(&no_parent, row(first), row(last));
                        }
                        apply_newest_first(change, &mut self.as_mut().rust_mut().messages);
                        // SAFETY: after `begin_insert_rows`.
                        unsafe { self.as_mut().end_insert_rows() };
                    }
                    MessageOp::Remove { row: at } => {
                        // SAFETY: balanced by `end_remove_rows` below.
                        unsafe {
                            self.as_mut()
                                .begin_remove_rows(&no_parent, row(at), row(at));
                        }
                        apply_newest_first(change, &mut self.as_mut().rust_mut().messages);
                        // SAFETY: after `begin_remove_rows`.
                        unsafe { self.as_mut().end_remove_rows() };
                    }
                    MessageOp::Update { first, last } => self.as_mut().rows_changed(first, last),
                    MessageOp::Replace { first, last } => {
                        apply_newest_first(change, &mut self.as_mut().rust_mut().messages);
                        self.as_mut().rows_changed(first, last);
                    }
                    MessageOp::Move { from, destination } => {
                        // SAFETY: `end_move_rows` is called below if this returns `true`.
                        let moving = unsafe {
                            self.as_mut().begin_move_rows(
                                &no_parent,
                                row(from),
                                row(from),
                                &no_parent,
                                row(destination),
                            )
                        };
                        apply_newest_first(change, &mut self.as_mut().rust_mut().messages);
                        if moving {
                            // SAFETY: after `begin_move_rows` returned `true`.
                            unsafe { self.as_mut().end_move_rows() };
                        }
                    }
                }
            }
        }
    }

    /// Shows the messages to someone else, with other custom emoji or under another policy: what
    /// they show, and what the viewer can do with them, changes.
    pub fn set_viewer(mut self: Pin<&mut Self>, viewer: Viewer) {
        if self.viewer == viewer {
            return;
        }
        self.as_mut().rust_mut().viewer = viewer;
        if let Some(last) = self.messages.len().checked_sub(1) {
            self.rows_changed(0, last);
        }
    }

    /// Takes new avatar files in and announces their role, and only that, for the rows that show
    /// them.
    pub fn set_avatars(mut self: Pin<&mut Self>, changes: &[(AvatarOwner, String)]) {
        self.as_mut().rust_mut().avatars.set(changes);
        let runs = rows_using(self.messages.len(), &owners(changes), |at| {
            self.messages.get(at).and_then(message_owner)
        });
        let no_parent = QModelIndex::default();
        for (first, last) in runs {
            let top_left = self.index(row(first), 0, &no_parent);
            let bottom_right = self.index(row(last), 0, &no_parent);
            let mut roles = QVector::<i32>::default();
            roles.append(AVATAR_ROLE);
            self.as_mut().data_changed(&top_left, &bottom_right, &roles);
        }
    }

    /// Takes new image files in and announces the attachments role, and only that, for the rows
    /// whose message shows them.
    pub fn set_images(mut self: Pin<&mut Self>, changes: &[(String, String)]) {
        self.as_mut().rust_mut().images.set(changes);
        let wanted = paths(changes);
        let rows = runs(self.messages.len(), |at| {
            self.messages
                .get(at)
                .is_some_and(|message| shows_image(message, &wanted))
        });
        let no_parent = QModelIndex::default();
        for (first, last) in rows {
            let top_left = self.index(row(first), 0, &no_parent);
            let bottom_right = self.index(row(last), 0, &no_parent);
            let mut roles = QVector::<i32>::default();
            roles.append(ATTACHMENTS_ROLE);
            self.as_mut().data_changed(&top_left, &bottom_right, &roles);
        }
    }

    /// Announces new content in every role of the rows `first..=last`.
    fn rows_changed(mut self: Pin<&mut Self>, first: usize, last: usize) {
        let no_parent = QModelIndex::default();
        let top_left = self.index(row(first), 0, &no_parent);
        let bottom_right = self.index(row(last), 0, &no_parent);
        // No roles listed means all of them.
        self.as_mut()
            .data_changed(&top_left, &bottom_right, &QVector::<i32>::default());
    }

    fn row_count(&self, _parent: &QModelIndex) -> i32 {
        row(self.messages.len())
    }

    fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(at) = usize::try_from(index.row()).ok() else {
            return QVariant::default();
        };
        let Some(message) = self.messages.get(at) else {
            return QVariant::default();
        };
        let text = |value: &str| QVariant::from(&QString::from(value));
        match role {
            MESSAGE_ID_ROLE => text(message.id.as_str()),
            AUTHOR_ROLE => text(author_name(&message.author)),
            USERNAME_ROLE => text(&message.author.username),
            TIME_ROLE => text(&time_text(message.ts, &self.zone)),
            DISPLAY_ROLE | BODY_ROLE => text(body(message)),
            IS_SYSTEM_ROLE => QVariant::from(&matches!(message.kind, MessageKind::System(_))),
            SYSTEM_TEXT_ROLE => text(&system_text(message)),
            FIRST_OF_DAY_ROLE => QVariant::from(&first_of_day(
                // The message before this one is the next row: rows run from newest to oldest.
                self.messages.get(at + 1),
                message,
                &self.zone,
            )),
            DAY_LABEL_ROLE => text(&day_label(message.ts, &self.zone, Timestamp::now())),
            DELIVERY_ROLE => text(delivery_name(message.delivery)),
            HTML_ROLE => text(&html(
                message,
                &self.viewer,
                Clock {
                    zone: &self.zone,
                    now: Timestamp::now(),
                },
            )),
            MENTIONS_ME_ROLE => QVariant::from(&mentions_me(message, &self.viewer)),
            AVATAR_ROLE => {
                text(message_owner(message).map_or("", |owner| self.avatars.url(&owner)))
            }
            AVATAR_INITIALS_ROLE => text(&initials(author_name(&message.author))),
            AVATAR_COLOR_ROLE => text(&color(&message.author.username)),
            ATTACHMENTS_ROLE => attachments_variant(&attachments(
                message,
                self.viewer.server.as_deref(),
                &self.images,
            )),
            EDITED_ROLE => QVariant::from(&message.edited),
            REACTIONS_ROLE => reactions_variant(&reactions(message, &self.viewer)),
            // Read at each call, so a time limit that runs out shows the next time the row is
            // drawn.
            CAN_EDIT_ROLE => QVariant::from(&can_edit(message, &self.viewer, Timestamp::now())),
            CAN_DELETE_ROLE => QVariant::from(&can_delete(message, &self.viewer, Timestamp::now())),
            QUOTES_ROLE => quotes_variant(&quotes(
                message,
                &self.viewer,
                Clock {
                    zone: &self.zone,
                    now: Timestamp::now(),
                },
            )),
            CAN_QUOTE_ROLE => QVariant::from(&can_quote(message)),
            CAN_REACT_ROLE => QVariant::from(&can_react(message)),
            _ => QVariant::default(),
        }
    }

    #[allow(clippy::unused_self)] // Signature fixed by `QAbstractItemModel::roleNames`.
    fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut names = QHash::<QHashPair_i32_QByteArray>::default();
        for (role, name) in [
            (DISPLAY_ROLE, "display"),
            (MESSAGE_ID_ROLE, "messageId"),
            (AUTHOR_ROLE, "author"),
            (USERNAME_ROLE, "username"),
            (TIME_ROLE, "time"),
            (BODY_ROLE, "body"),
            (IS_SYSTEM_ROLE, "isSystem"),
            (SYSTEM_TEXT_ROLE, "systemText"),
            (FIRST_OF_DAY_ROLE, "firstOfDay"),
            (DAY_LABEL_ROLE, "dayLabel"),
            (DELIVERY_ROLE, "delivery"),
            (HTML_ROLE, "html"),
            (MENTIONS_ME_ROLE, "mentionsMe"),
            (AVATAR_ROLE, "avatar"),
            (AVATAR_INITIALS_ROLE, "avatarInitials"),
            (AVATAR_COLOR_ROLE, "avatarColor"),
            (ATTACHMENTS_ROLE, "attachments"),
            (EDITED_ROLE, "edited"),
            (REACTIONS_ROLE, "reactions"),
            (CAN_EDIT_ROLE, "canEdit"),
            (CAN_DELETE_ROLE, "canDelete"),
            (QUOTES_ROLE, "quotes"),
            (CAN_QUOTE_ROLE, "canQuote"),
            (CAN_REACT_ROLE, "canReact"),
        ] {
            names.insert(role, QByteArray::from(name));
        }
        names
    }
}

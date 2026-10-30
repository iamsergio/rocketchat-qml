// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What the `MessageList` and `ThreadMessageList` models share: the roles the views read from an
//! `rc_view` message list, and the macro that makes the models' bridges apply the core's changes
//! to it. The models are thin: a bridge, the notifications Qt wants around each change, and this.

use cxx_qt_lib::{
    QByteArray, QHash, QHashPair_i32_QByteArray, QList, QMap, QMapPair_QString_QVariant, QString,
    QVariant,
};
use jiff::Timestamp;
use rc_core::domain::MessageKind;

use rc_view::attachment_view::{AttachmentView, attachments};
use rc_view::avatar_view::{color, initials, message_owner};
use rc_view::message_list::MessageList;
use rc_view::message_view::{
    author_name, body, can_delete, can_edit, can_quote, can_react, can_start_thread, day_label,
    delivery_name, in_thread_shown, mentions_me, system_text, thread_id, thread_last_reply_text,
    thread_replies, time_text,
};
use rc_view::quote_view::{QuoteView, quotes};
use rc_view::reaction_view::{ReactionView, reactions};
use rc_view::rich_html::{message_html, render};
use rc_view::viewer::{Clock, Viewer};

/// `Qt::DisplayRole`, which shows the body too.
const DISPLAY_ROLE: i32 = 0;

// The custom roles, from `Qt::UserRole` on (see `RoomListModel`).
pub const MESSAGE_ID_ROLE: i32 = 256;
pub const AUTHOR_ROLE: i32 = 257;
pub const USERNAME_ROLE: i32 = 258;
pub const TIME_ROLE: i32 = 259;
pub const BODY_ROLE: i32 = 260;
pub const IS_SYSTEM_ROLE: i32 = 261;
pub const SYSTEM_TEXT_ROLE: i32 = 262;
pub const FIRST_OF_DAY_ROLE: i32 = 263;
pub const DAY_LABEL_ROLE: i32 = 264;
pub const DELIVERY_ROLE: i32 = 265;
pub const HTML_ROLE: i32 = 266;
pub const MENTIONS_ME_ROLE: i32 = 267;
pub const AVATAR_ROLE: i32 = 268;
pub const AVATAR_INITIALS_ROLE: i32 = 269;
pub const AVATAR_COLOR_ROLE: i32 = 270;
pub const ATTACHMENTS_ROLE: i32 = 271;
pub const EDITED_ROLE: i32 = 272;
pub const REACTIONS_ROLE: i32 = 273;
pub const CAN_EDIT_ROLE: i32 = 274;
pub const CAN_DELETE_ROLE: i32 = 275;
pub const QUOTES_ROLE: i32 = 276;
pub const CAN_QUOTE_ROLE: i32 = 277;
pub const CAN_REACT_ROLE: i32 = 278;
pub const THREAD_REPLIES_ROLE: i32 = 279;
pub const THREAD_LAST_REPLY_ROLE: i32 = 280;
pub const THREAD_ID_ROLE: i32 = 281;
pub const IN_THREAD_SHOWN_ROLE: i32 = 282;
pub const CAN_START_THREAD_ROLE: i32 = 283;
pub const THREAD_PARENT_TEXT_ROLE: i32 = 284;
/// The message is the one the room was asked to show (a search result): marked.
pub const FOUND_ROLE: i32 = 285;

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
fn quotes_variant(views: &[QuoteView], viewer: &Viewer, clock: Clock) -> QVariant {
    let text = |value: &str| QVariant::from(&QString::from(value));
    let mut list = QList::<QVariant>::default();
    for view in views {
        let mut map = QMap::<QMapPair_QString_QVariant>::default();
        for (key, value) in [
            ("author", text(&view.author)),
            ("bodyHtml", text(&render(&view.text, viewer, clock))),
            ("body", text(&view.body)),
            ("time", text(&view.time)),
        ] {
            map.insert(QString::from(key), value);
        }
        list.append(QVariant::from(&map));
    }
    QVariant::from(&list)
}

/// `index` as the `int` Qt wants. A room never comes near `i32::MAX` messages.
pub fn row(index: usize) -> i32 {
    i32::try_from(index).unwrap_or(i32::MAX)
}

/// The value of `role` for the message at `row` of `list`.
#[must_use]
pub fn data(list: &MessageList, row: i32, role: i32) -> QVariant {
    let Some(at) = usize::try_from(row).ok() else {
        return QVariant::default();
    };
    let Some(message) = list.message(at) else {
        return QVariant::default();
    };
    let text = |value: &str| QVariant::from(&QString::from(value));
    match role {
        MESSAGE_ID_ROLE => text(message.id.as_str()),
        AUTHOR_ROLE => text(author_name(&message.author)),
        USERNAME_ROLE => text(&message.author.username),
        TIME_ROLE => text(&time_text(message.ts, list.zone())),
        DISPLAY_ROLE | BODY_ROLE => text(body(message)),
        IS_SYSTEM_ROLE => QVariant::from(&matches!(message.kind, MessageKind::System(_))),
        SYSTEM_TEXT_ROLE => text(&system_text(message)),
        FIRST_OF_DAY_ROLE => QVariant::from(&list.first_of_day(at)),
        DAY_LABEL_ROLE => text(&day_label(message.ts, list.zone(), Timestamp::now())),
        DELIVERY_ROLE => text(delivery_name(message.delivery)),
        HTML_ROLE => text(&message_html(
            message,
            list.viewer(),
            Clock {
                zone: list.zone(),
                now: Timestamp::now(),
            },
        )),
        MENTIONS_ME_ROLE => QVariant::from(&mentions_me(message, list.viewer())),
        AVATAR_ROLE => text(message_owner(message).map_or("", |owner| list.avatars().url(&owner))),
        AVATAR_INITIALS_ROLE => text(&initials(author_name(&message.author))),
        AVATAR_COLOR_ROLE => text(&color(&message.author.username)),
        ATTACHMENTS_ROLE => attachments_variant(&attachments(
            message,
            list.viewer().server.as_deref(),
            list.images(),
        )),
        EDITED_ROLE => QVariant::from(&message.edited),
        REACTIONS_ROLE => reactions_variant(&reactions(message, list.viewer())),
        // Read at each call, so a time limit that runs out shows the next time the row is
        // drawn.
        CAN_EDIT_ROLE => QVariant::from(&can_edit(message, list.viewer(), Timestamp::now())),
        CAN_DELETE_ROLE => QVariant::from(&can_delete(message, list.viewer(), Timestamp::now())),
        QUOTES_ROLE => {
            let clock = Clock {
                zone: list.zone(),
                now: Timestamp::now(),
            };
            quotes_variant(&quotes(message, clock), list.viewer(), clock)
        }
        CAN_QUOTE_ROLE => QVariant::from(&can_quote(message)),
        CAN_REACT_ROLE => QVariant::from(&can_react(message)),
        THREAD_REPLIES_ROLE => QVariant::from(&thread_replies(message)),
        THREAD_LAST_REPLY_ROLE => text(&thread_last_reply_text(
            message,
            list.zone(),
            Timestamp::now(),
        )),
        THREAD_ID_ROLE => text(thread_id(message)),
        IN_THREAD_SHOWN_ROLE => QVariant::from(&in_thread_shown(message)),
        CAN_START_THREAD_ROLE => QVariant::from(&can_start_thread(message)),
        THREAD_PARENT_TEXT_ROLE => text(&list.thread_parent_text(at)),
        FOUND_ROLE => QVariant::from(&list.is_found(at)),
        _ => QVariant::default(),
    }
}

/// The names QML reads the roles by.
#[must_use]
pub fn role_names() -> QHash<QHashPair_i32_QByteArray> {
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
        (THREAD_REPLIES_ROLE, "threadReplies"),
        (THREAD_LAST_REPLY_ROLE, "threadLastReply"),
        (THREAD_ID_ROLE, "threadId"),
        (IN_THREAD_SHOWN_ROLE, "inThreadShown"),
        (CAN_START_THREAD_ROLE, "canStartThread"),
        (THREAD_PARENT_TEXT_ROLE, "threadParentText"),
        (FOUND_ROLE, "found"),
    ] {
        names.insert(role, QByteArray::from(name));
    }
    names
}

/// Makes a model bridge apply the core's changes to its `mirror` (a [`MessageList`] field of its
/// Rust struct), announcing each one to the views, and read the roles from it. The bridge
/// declares the `begin*` and `end*` calls, the `dataChanged` signal and `index`, whose types are
/// its own, and `row_count`, `data` and `role_names` as overrides of the base class.
macro_rules! message_model {
    ($model:ident) => {
        impl $model {
            /// Applies the changes in order, announcing each one to the views.
            pub fn apply(
                mut self: ::core::pin::Pin<&mut Self>,
                changes: &[::rc_core::timeline::TimelineChange],
            ) {
                use ::cxx_qt::CxxQtType;
                use ::rc_view::row_ops::MessageOp;
                use $crate::message_mirror::row;

                let no_parent = ::cxx_qt_lib::QModelIndex::default();
                for change in changes {
                    for op in self.mirror.ops(change) {
                        match op {
                            MessageOp::Reset => {
                                // SAFETY: balanced by `end_reset_model` below.
                                unsafe { self.as_mut().begin_reset_model() };
                                self.as_mut().rust_mut().mirror.apply(change);
                                // SAFETY: after `begin_reset_model`.
                                unsafe { self.as_mut().end_reset_model() };
                            }
                            MessageOp::Insert { first, last } => {
                                // SAFETY: balanced by `end_insert_rows` below.
                                unsafe {
                                    self.as_mut().begin_insert_rows(
                                        &no_parent,
                                        row(first),
                                        row(last),
                                    );
                                }
                                self.as_mut().rust_mut().mirror.apply(change);
                                // SAFETY: after `begin_insert_rows`.
                                unsafe { self.as_mut().end_insert_rows() };
                            }
                            MessageOp::Remove { row: at } => {
                                // SAFETY: balanced by `end_remove_rows` below.
                                unsafe {
                                    self.as_mut()
                                        .begin_remove_rows(&no_parent, row(at), row(at));
                                }
                                self.as_mut().rust_mut().mirror.apply(change);
                                // SAFETY: after `begin_remove_rows`.
                                unsafe { self.as_mut().end_remove_rows() };
                            }
                            MessageOp::Update { first, last } => {
                                self.as_mut().rows_changed(first, last);
                            }
                            MessageOp::Replace { first, last } => {
                                self.as_mut().rust_mut().mirror.apply(change);
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
                                self.as_mut().rust_mut().mirror.apply(change);
                                if moving {
                                    // SAFETY: after `begin_move_rows` returned `true`.
                                    unsafe { self.as_mut().end_move_rows() };
                                }
                            }
                        }
                    }
                }
            }

            /// Replaces everything shown with another account's (see
            /// [`MessageList::replace`]), as one model reset.
            pub fn show(
                mut self: ::core::pin::Pin<&mut Self>,
                snapshot: &::rc_core::timeline::TimelineChange,
                viewer: ::rc_view::viewer::Viewer,
                avatars: ::rc_view::avatar_view::AvatarUrls,
                images: ::rc_view::attachment_view::ImageUrls,
            ) {
                use ::cxx_qt::CxxQtType;

                // SAFETY: balanced by `end_reset_model` below.
                unsafe { self.as_mut().begin_reset_model() };
                self.as_mut()
                    .rust_mut()
                    .mirror
                    .replace(snapshot, viewer, avatars, images);
                // SAFETY: after `begin_reset_model`.
                unsafe { self.as_mut().end_reset_model() };
            }

            /// Shows the messages to someone else (see [`MessageList::set_viewer`]).
            pub fn set_viewer(
                mut self: ::core::pin::Pin<&mut Self>,
                viewer: ::rc_view::viewer::Viewer,
            ) {
                use ::cxx_qt::CxxQtType;

                if let Some((first, last)) = self.as_mut().rust_mut().mirror.set_viewer(viewer) {
                    self.rows_changed(first, last);
                }
            }

            /// Takes new avatar files in and announces their role, and only that, for the rows
            /// that show them.
            pub fn set_avatars(
                mut self: ::core::pin::Pin<&mut Self>,
                changes: &[(::rc_core::domain::AvatarOwner, String)],
            ) {
                use ::cxx_qt::CxxQtType;

                let runs = self.as_mut().rust_mut().mirror.set_avatars(changes);
                self.role_changed(&runs, $crate::message_mirror::AVATAR_ROLE);
            }

            /// Takes new image files in and announces the attachments role, and only that, for
            /// the rows whose message shows them.
            pub fn set_images(mut self: ::core::pin::Pin<&mut Self>, changes: &[(String, String)]) {
                use ::cxx_qt::CxxQtType;

                let runs = self.as_mut().rust_mut().mirror.set_images(changes);
                self.role_changed(&runs, $crate::message_mirror::ATTACHMENTS_ROLE);
            }

            /// Announces new content of `role`, and only that, in the runs of rows.
            fn role_changed(
                mut self: ::core::pin::Pin<&mut Self>,
                runs: &[(usize, usize)],
                role: i32,
            ) {
                use $crate::message_mirror::row;

                let no_parent = ::cxx_qt_lib::QModelIndex::default();
                for &(first, last) in runs {
                    let top_left = self.index(row(first), 0, &no_parent);
                    let bottom_right = self.index(row(last), 0, &no_parent);
                    let mut roles = ::cxx_qt_lib::QVector::<i32>::default();
                    roles.append(role);
                    self.as_mut().data_changed(&top_left, &bottom_right, &roles);
                }
            }

            /// Announces new content in every role of the rows `first..=last`.
            fn rows_changed(mut self: ::core::pin::Pin<&mut Self>, first: usize, last: usize) {
                use $crate::message_mirror::row;

                let no_parent = ::cxx_qt_lib::QModelIndex::default();
                let top_left = self.index(row(first), 0, &no_parent);
                let bottom_right = self.index(row(last), 0, &no_parent);
                // No roles listed means all of them.
                self.as_mut().data_changed(
                    &top_left,
                    &bottom_right,
                    &::cxx_qt_lib::QVector::<i32>::default(),
                );
            }

            fn row_count(&self, _parent: &::cxx_qt_lib::QModelIndex) -> i32 {
                use $crate::message_mirror::row;

                row(self.mirror.row_count())
            }

            fn data(&self, index: &::cxx_qt_lib::QModelIndex, role: i32) -> ::cxx_qt_lib::QVariant {
                $crate::message_mirror::data(&self.mirror, index.row(), role)
            }

            fn role_names(&self) -> ::cxx_qt_lib::QHash<::cxx_qt_lib::QHashPair_i32_QByteArray> {
                $crate::message_mirror::role_names()
            }
        }
    };
}
pub(crate) use message_model;

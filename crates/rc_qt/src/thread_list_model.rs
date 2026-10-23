// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The `ThreadListModel` model: the parents of the open room's threads, the one with the most
//! recent reply first, held and announced as `RoomList` holds and announces the rooms (inserts,
//! updates and moves of single rows, a reset when the list opens, closes or changes its filter).

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
        /// Available in QML as the `ThreadListModel` singleton.
        #[qobject]
        #[qml_element = "ThreadListModel"]
        #[qml_singleton]
        #[base = QAbstractListModel]
        type ThreadListModel = super::ThreadListModelRust;

        #[qsignal]
        #[inherit]
        #[cxx_name = "dataChanged"]
        fn data_changed(
            self: Pin<&mut ThreadListModel>,
            top_left: &QModelIndex,
            bottom_right: &QModelIndex,
            roles: &QVector_i32,
        );

        #[inherit]
        fn index(
            self: &ThreadListModel,
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
            self: Pin<&mut ThreadListModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        /// # Safety
        ///
        /// Only after `begin_insert_rows`.
        #[inherit]
        #[cxx_name = "endInsertRows"]
        unsafe fn end_insert_rows(self: Pin<&mut ThreadListModel>);

        /// # Safety
        ///
        /// Call `end_remove_rows` after changing the data.
        #[inherit]
        #[cxx_name = "beginRemoveRows"]
        unsafe fn begin_remove_rows(
            self: Pin<&mut ThreadListModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        /// # Safety
        ///
        /// Only after `begin_remove_rows`.
        #[inherit]
        #[cxx_name = "endRemoveRows"]
        unsafe fn end_remove_rows(self: Pin<&mut ThreadListModel>);

        /// Returns `false` if the move is invalid, in which case there is nothing to end.
        ///
        /// # Safety
        ///
        /// If it returns `true`, call `end_move_rows` after changing the data.
        #[inherit]
        #[cxx_name = "beginMoveRows"]
        unsafe fn begin_move_rows(
            self: Pin<&mut ThreadListModel>,
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
        unsafe fn end_move_rows(self: Pin<&mut ThreadListModel>);

        /// # Safety
        ///
        /// Call `end_reset_model` after changing the data.
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut ThreadListModel>);
        /// # Safety
        ///
        /// Only after `begin_reset_model`.
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut ThreadListModel>);
    }

    extern "RustQt" {
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &ThreadListModel, parent: &QModelIndex) -> i32;

        #[cxx_override]
        fn data(self: &ThreadListModel, index: &QModelIndex, role: i32) -> QVariant;

        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &ThreadListModel) -> QHash_i32_QByteArray;
    }

    impl cxx_qt::Threading for ThreadListModel {}
    impl cxx_qt::Initialize for ThreadListModel {}
}

use core::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant, QVector};
use jiff::Timestamp;
use jiff::tz::TimeZone;
use rc_core::domain::{AvatarOwner, Message};
use rc_core::threads::ThreadListChange;

use crate::hub;
use crate::room_roles::row;
use rc_view::avatar_view::{AvatarUrls, color, initials, message_owner, owners, rows_using};
use rc_view::message_view::{author_name, thread_last_reply_text};
use rc_view::row_ops::{RowOp, row_ops};
use rc_view::thread_list_view::{ThreadUnread, row_summary, row_text, rows_changed};

use qobject::ThreadListModel;

/// `Qt::DisplayRole`, which shows the text too.
const DISPLAY_ROLE: i32 = 0;

// The custom roles, from `Qt::UserRole` on (see `RoomListModel`).
const THREAD_ID_ROLE: i32 = 256;
const AUTHOR_ROLE: i32 = 257;
const LAST_REPLY_ROLE: i32 = 258;
const TEXT_ROLE: i32 = 259;
const REPLIES_ROLE: i32 = 260;
const SUMMARY_ROLE: i32 = 261;
/// The only role that changes when an avatar file arrives.
const AVATAR_ROLE: i32 = 262;
const AVATAR_INITIALS_ROLE: i32 = 263;
const AVATAR_COLOR_ROLE: i32 = 264;
/// The two that change when the subscription says other threads are unread.
const UNREAD_ROLE: i32 = 265;
const MENTIONED_ROLE: i32 = 266;

/// Backing data of [`qobject::ThreadListModel`]: the parents of the threads, mirrored from the
/// core's changes.
pub struct ThreadListModelRust {
    threads: Vec<Message>,
    avatars: AvatarUrls,
    /// Which threads have replies the user hasn't read.
    unread: ThreadUnread,
    /// Where times and days are read: the user's.
    zone: TimeZone,
}

impl Default for ThreadListModelRust {
    fn default() -> Self {
        Self {
            threads: Vec::new(),
            avatars: AvatarUrls::default(),
            unread: ThreadUnread::default(),
            zone: TimeZone::system(),
        }
    }
}

impl cxx_qt::Initialize for ThreadListModel {
    fn initialize(mut self: Pin<&mut Self>) {
        let (snapshot, avatars, unread) = hub::register_thread_list_model(self.qt_thread());
        self.as_mut().rust_mut().avatars = avatars;
        self.as_mut().rust_mut().unread = unread;
        self.as_mut().apply(&[snapshot]);
    }
}

impl ThreadListModel {
    /// Replaces the threads and the avatars with another account's, as one model reset.
    pub fn show(
        mut self: Pin<&mut Self>,
        snapshot: ThreadListChange,
        avatars: AvatarUrls,
        unread: ThreadUnread,
    ) {
        self.as_mut().rust_mut().avatars = avatars;
        self.as_mut().rust_mut().unread = unread;
        self.apply(&[snapshot]);
    }

    /// Applies the changes in order, announcing each one to the views.
    pub fn apply(mut self: Pin<&mut Self>, changes: &[ThreadListChange]) {
        let no_parent = QModelIndex::default();
        for change in changes {
            match row_ops(change) {
                RowOp::Reset => {
                    // SAFETY: balanced by `end_reset_model` below.
                    unsafe { self.as_mut().begin_reset_model() };
                    change.apply(&mut self.as_mut().rust_mut().threads);
                    // SAFETY: after `begin_reset_model`.
                    unsafe { self.as_mut().end_reset_model() };
                }
                RowOp::Insert { row: at } => {
                    // SAFETY: balanced by `end_insert_rows` below.
                    unsafe {
                        self.as_mut()
                            .begin_insert_rows(&no_parent, row(at), row(at));
                    }
                    change.apply(&mut self.as_mut().rust_mut().threads);
                    // SAFETY: after `begin_insert_rows`.
                    unsafe { self.as_mut().end_insert_rows() };
                }
                RowOp::Update { row: at } => {
                    change.apply(&mut self.as_mut().rust_mut().threads);
                    self.as_mut().row_changed(at);
                }
                RowOp::Remove { row: at } => {
                    // SAFETY: balanced by `end_remove_rows` below.
                    unsafe {
                        self.as_mut()
                            .begin_remove_rows(&no_parent, row(at), row(at));
                    }
                    change.apply(&mut self.as_mut().rust_mut().threads);
                    // SAFETY: after `begin_remove_rows`.
                    unsafe { self.as_mut().end_remove_rows() };
                }
                RowOp::Move {
                    from,
                    destination,
                    to,
                } => {
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
                    change.apply(&mut self.as_mut().rust_mut().threads);
                    if moving {
                        // SAFETY: after `begin_move_rows` returned `true`.
                        unsafe { self.as_mut().end_move_rows() };
                    }
                    // The content may have changed along with the position.
                    self.as_mut().row_changed(to);
                }
            }
        }
    }

    /// Takes new avatar files in and announces their role, and only that, for the rows that show
    /// them.
    pub fn set_avatars(mut self: Pin<&mut Self>, changes: &[(AvatarOwner, String)]) {
        self.as_mut().rust_mut().avatars.set(changes);
        let runs = rows_using(self.threads.len(), &owners(changes), |at| {
            self.threads.get(at).and_then(message_owner)
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

    /// Takes in which threads are unread, and announces the two roles that say so, and only
    /// those, for the rows that look different.
    pub fn set_unread(mut self: Pin<&mut Self>, unread: ThreadUnread) {
        let runs = rows_changed(&self.threads, &self.unread, &unread);
        self.as_mut().rust_mut().unread = unread;
        let no_parent = QModelIndex::default();
        for (first, last) in runs {
            let top_left = self.index(row(first), 0, &no_parent);
            let bottom_right = self.index(row(last), 0, &no_parent);
            let mut roles = QVector::<i32>::default();
            roles.append(UNREAD_ROLE);
            roles.append(MENTIONED_ROLE);
            self.as_mut().data_changed(&top_left, &bottom_right, &roles);
        }
    }

    /// Announces new content in every role of the row.
    fn row_changed(mut self: Pin<&mut Self>, at: usize) {
        let index = self.index(row(at), 0, &QModelIndex::default());
        // No roles listed means all of them.
        self.as_mut()
            .data_changed(&index, &index, &QVector::<i32>::default());
    }

    fn row_count(&self, _parent: &QModelIndex) -> i32 {
        row(self.threads.len())
    }

    fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(thread) = usize::try_from(index.row())
            .ok()
            .and_then(|at| self.threads.get(at))
        else {
            return QVariant::default();
        };
        let text = |value: &str| QVariant::from(&QString::from(value));
        let now = Timestamp::now();
        match role {
            THREAD_ID_ROLE => text(thread.id.as_str()),
            AUTHOR_ROLE => text(author_name(&thread.author)),
            LAST_REPLY_ROLE => text(&thread_last_reply_text(thread, &self.zone, now)),
            DISPLAY_ROLE | TEXT_ROLE => text(&row_text(thread)),
            REPLIES_ROLE => {
                QVariant::from(&i32::try_from(thread.thread_replies).unwrap_or(i32::MAX))
            }
            SUMMARY_ROLE => text(&row_summary(thread)),
            AVATAR_ROLE => text(message_owner(thread).map_or("", |owner| self.avatars.url(&owner))),
            AVATAR_INITIALS_ROLE => text(&initials(author_name(&thread.author))),
            AVATAR_COLOR_ROLE => text(&color(&thread.author.username)),
            UNREAD_ROLE => QVariant::from(&self.unread.is_unread(&thread.id)),
            MENTIONED_ROLE => QVariant::from(&self.unread.is_mentioned(&thread.id)),
            _ => QVariant::default(),
        }
    }

    #[allow(clippy::unused_self)] // Signature fixed by `QAbstractItemModel::roleNames`.
    fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut names = QHash::<QHashPair_i32_QByteArray>::default();
        for (role, name) in [
            (DISPLAY_ROLE, "display"),
            (THREAD_ID_ROLE, "threadId"),
            (AUTHOR_ROLE, "author"),
            (LAST_REPLY_ROLE, "lastReply"),
            (TEXT_ROLE, "body"),
            (REPLIES_ROLE, "replies"),
            (SUMMARY_ROLE, "summary"),
            (AVATAR_ROLE, "avatar"),
            (AVATAR_INITIALS_ROLE, "avatarInitials"),
            (AVATAR_COLOR_ROLE, "avatarColor"),
            (UNREAD_ROLE, "unread"),
            (MENTIONED_ROLE, "mentioned"),
        ] {
            names.insert(role, cxx_qt_lib::QByteArray::from(name));
        }
        names
    }
}

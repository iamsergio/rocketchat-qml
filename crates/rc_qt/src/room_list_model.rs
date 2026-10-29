// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The `RoomList` model: the rooms in the core's order, updated one row at a time.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++Qt" {
        include!(<QtCore/QAbstractListModel>);
        #[qobject]
        type QAbstractListModel;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

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
        /// Available in QML as the `RoomList` singleton.
        #[qobject]
        #[qml_element = "RoomList"]
        #[qml_singleton]
        #[base = QAbstractListModel]
        // Whether any room is a favorite: the list shows its sections only then.
        #[qproperty(bool, has_favorites, cxx_name = "hasFavorites", READ, NOTIFY)]
        // Why the last command on a room (a favorite, mark as read or unread, leaving) failed, to
        // show until it is dismissed. Cleared by the next command and by `dismissRoomActionError`.
        #[qproperty(QString, room_action_error, cxx_name = "roomActionError", READ, NOTIFY)]
        type RoomListModel = super::RoomListModelRust;

        /// Marks the room as a favorite, or takes it back. The room moves in the list before the
        /// answer comes; a failure is in `roomActionError`.
        #[qinvokable]
        #[cxx_name = "setFavorite"]
        fn set_favorite(self: Pin<&mut RoomListModel>, room_id: &QString, favorite: bool);

        /// Marks the room read, on the server and in the list.
        #[qinvokable]
        #[cxx_name = "markRead"]
        fn mark_read(self: Pin<&mut RoomListModel>, room_id: &QString);

        /// Marks the room unread. The open room is refused (it stays read while open), and so is
        /// a room without messages.
        #[qinvokable]
        #[cxx_name = "markUnread"]
        fn mark_unread(self: Pin<&mut RoomListModel>, room_id: &QString);

        /// Leaves a channel or a private group. The row leaves the list when the server agrees.
        #[qinvokable]
        fn leave(self: Pin<&mut RoomListModel>, room_id: &QString);

        /// F5: shows the channel news the batch holds back, of every account (ROOM-012). Emits
        /// `nothingHeld` if no account holds any. This is the room list's, as it is the rows that
        /// change.
        #[qinvokable]
        #[cxx_name = "releaseNews"]
        fn release_news(self: Pin<&mut RoomListModel>);

        /// F5 found nothing held anywhere: the page says so.
        #[qsignal]
        #[cxx_name = "nothingHeld"]
        fn nothing_held(self: Pin<&mut RoomListModel>);

        /// Clears `roomActionError`.
        #[qinvokable]
        #[cxx_name = "dismissRoomActionError"]
        fn dismiss_room_action_error(self: Pin<&mut RoomListModel>);

        #[qsignal]
        #[inherit]
        #[cxx_name = "dataChanged"]
        fn data_changed(
            self: Pin<&mut RoomListModel>,
            top_left: &QModelIndex,
            bottom_right: &QModelIndex,
            roles: &QVector_i32,
        );

        #[inherit]
        fn index(self: &RoomListModel, row: i32, column: i32, parent: &QModelIndex) -> QModelIndex;
    }

    // The begin/end pairs must be kept balanced, so they are unsafe to call.
    extern "RustQt" {
        /// # Safety
        ///
        /// Call `end_insert_rows` after changing the data.
        #[inherit]
        #[cxx_name = "beginInsertRows"]
        unsafe fn begin_insert_rows(
            self: Pin<&mut RoomListModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        /// # Safety
        ///
        /// Only after `begin_insert_rows`.
        #[inherit]
        #[cxx_name = "endInsertRows"]
        unsafe fn end_insert_rows(self: Pin<&mut RoomListModel>);

        /// # Safety
        ///
        /// Call `end_remove_rows` after changing the data.
        #[inherit]
        #[cxx_name = "beginRemoveRows"]
        unsafe fn begin_remove_rows(
            self: Pin<&mut RoomListModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        /// # Safety
        ///
        /// Only after `begin_remove_rows`.
        #[inherit]
        #[cxx_name = "endRemoveRows"]
        unsafe fn end_remove_rows(self: Pin<&mut RoomListModel>);

        /// Returns `false` if the move is invalid, in which case there is nothing to end.
        ///
        /// # Safety
        ///
        /// If it returns `true`, call `end_move_rows` after changing the data.
        #[inherit]
        #[cxx_name = "beginMoveRows"]
        unsafe fn begin_move_rows(
            self: Pin<&mut RoomListModel>,
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
        unsafe fn end_move_rows(self: Pin<&mut RoomListModel>);

        /// # Safety
        ///
        /// Call `end_reset_model` after changing the data.
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut RoomListModel>);
        /// # Safety
        ///
        /// Only after `begin_reset_model`.
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut RoomListModel>);
    }

    extern "RustQt" {
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &RoomListModel, parent: &QModelIndex) -> i32;

        #[cxx_override]
        fn data(self: &RoomListModel, index: &QModelIndex, role: i32) -> QVariant;

        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &RoomListModel) -> QHash_i32_QByteArray;
    }

    impl cxx_qt::Threading for RoomListModel {}
    impl cxx_qt::Initialize for RoomListModel {}
}

use core::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant, QVector};
use rc_core::domain::{AvatarOwner, Room};
use rc_core::rooms::{RoomActionError, RoomChange};

use crate::hub;
use crate::room_roles::{AVATAR_ROLE, room_data, room_role_names, row};
use rc_view::avatar_view::{AvatarUrls, owners, rows_using};
use rc_view::room_view::any_favorite;
use rc_view::row_ops::{RowOp, row_ops};

use qobject::RoomListModel;

/// Backing data of [`qobject::RoomListModel`]: the rooms, mirrored from the core's changes.
#[derive(Default)]
pub struct RoomListModelRust {
    rooms: Vec<Room>,
    avatars: AvatarUrls,
    has_favorites: bool,
    room_action_error: QString,
}

impl cxx_qt::Initialize for RoomListModel {
    fn initialize(mut self: Pin<&mut Self>) {
        let (snapshot, avatars) = hub::register_rooms(self.qt_thread());
        self.as_mut().rust_mut().avatars = avatars;
        self.as_mut().apply(&[snapshot]);
    }
}

impl RoomListModel {
    /// Replaces the rooms and their avatars with another account's, as one model reset. What
    /// failed for the other account means nothing for this one.
    pub fn show(mut self: Pin<&mut Self>, snapshot: RoomChange, avatars: AvatarUrls) {
        self.as_mut().rust_mut().avatars = avatars;
        self.as_mut().set_action_error_text("");
        self.apply(&[snapshot]);
    }

    /// Applies the changes in order, announcing each one to the views, then `hasFavorites` if it
    /// changed.
    pub fn apply(mut self: Pin<&mut Self>, changes: &[RoomChange]) {
        self.as_mut().apply_changes(changes);
        let has_favorites = any_favorite(&self.rooms);
        if self.has_favorites != has_favorites {
            self.as_mut().rust_mut().has_favorites = has_favorites;
            self.has_favorites_changed();
        }
    }

    fn apply_changes(mut self: Pin<&mut Self>, changes: &[RoomChange]) {
        let no_parent = QModelIndex::default();
        for change in changes {
            match row_ops(change) {
                RowOp::Reset => {
                    // SAFETY: balanced by `end_reset_model` below.
                    unsafe { self.as_mut().begin_reset_model() };
                    change.apply(&mut self.as_mut().rust_mut().rooms);
                    // SAFETY: after `begin_reset_model`.
                    unsafe { self.as_mut().end_reset_model() };
                }
                RowOp::Insert { row: at } => {
                    // SAFETY: balanced by `end_insert_rows` below.
                    unsafe {
                        self.as_mut()
                            .begin_insert_rows(&no_parent, row(at), row(at));
                    }
                    change.apply(&mut self.as_mut().rust_mut().rooms);
                    // SAFETY: after `begin_insert_rows`.
                    unsafe { self.as_mut().end_insert_rows() };
                }
                RowOp::Update { row: at } => {
                    change.apply(&mut self.as_mut().rust_mut().rooms);
                    self.as_mut().row_changed(at);
                }
                RowOp::Remove { row: at } => {
                    // SAFETY: balanced by `end_remove_rows` below.
                    unsafe {
                        self.as_mut()
                            .begin_remove_rows(&no_parent, row(at), row(at));
                    }
                    change.apply(&mut self.as_mut().rust_mut().rooms);
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
                    change.apply(&mut self.as_mut().rust_mut().rooms);
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

    /// Takes the core's answer to a command on a room in: a failure becomes `roomActionError`.
    pub fn finish_action(self: Pin<&mut Self>, outcome: &Result<(), RoomActionError>) {
        let error_text = outcome
            .as_ref()
            .err()
            .map_or_else(String::new, ToString::to_string);
        self.set_action_error_text(&error_text);
    }

    /// Sets `roomActionError`, announcing it if it changed.
    fn set_action_error_text(mut self: Pin<&mut Self>, text: &str) {
        let text = QString::from(text);
        if self.room_action_error != text {
            self.as_mut().rust_mut().room_action_error = text;
            self.room_action_error_changed();
        }
    }

    fn set_favorite(mut self: Pin<&mut Self>, room_id: &QString, favorite: bool) {
        self.as_mut().set_action_error_text("");
        hub::set_favorite(room_id.to_string(), favorite);
    }

    fn mark_read(mut self: Pin<&mut Self>, room_id: &QString) {
        self.as_mut().set_action_error_text("");
        hub::mark_room_read(room_id.to_string());
    }

    fn mark_unread(mut self: Pin<&mut Self>, room_id: &QString) {
        self.as_mut().set_action_error_text("");
        hub::mark_room_unread(room_id.to_string());
    }

    fn leave(mut self: Pin<&mut Self>, room_id: &QString) {
        self.as_mut().set_action_error_text("");
        hub::leave_room(room_id.to_string());
    }

    fn release_news(self: Pin<&mut Self>) {
        tracing::info!("F5 pressed");
        if !hub::release_news() {
            self.nothing_held();
        }
    }

    fn dismiss_room_action_error(self: Pin<&mut Self>) {
        self.set_action_error_text("");
    }

    /// Takes new avatar files in and announces their role, and only that, for the rows that show
    /// them.
    pub fn set_avatars(mut self: Pin<&mut Self>, changes: &[(AvatarOwner, String)]) {
        self.as_mut().rust_mut().avatars.set(changes);
        let runs = rows_using(self.rooms.len(), &owners(changes), |at| {
            self.rooms.get(at).map(Room::avatar_owner)
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

    /// Announces new content in every role of the row.
    fn row_changed(mut self: Pin<&mut Self>, at: usize) {
        let index = self.index(row(at), 0, &QModelIndex::default());
        // No roles listed means all of them.
        self.as_mut()
            .data_changed(&index, &index, &QVector::<i32>::default());
    }

    fn row_count(&self, _parent: &QModelIndex) -> i32 {
        row(self.rooms.len())
    }

    fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(room) = usize::try_from(index.row())
            .ok()
            .and_then(|at| self.rooms.get(at))
        else {
            return QVariant::default();
        };
        room_data(room, &self.avatars, role)
    }

    #[allow(clippy::unused_self)] // Signature fixed by `QAbstractItemModel::roleNames`.
    fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        room_role_names()
    }
}

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
        type RoomListModel = super::RoomListModelRust;

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
use cxx_qt_lib::{
    QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant, QVector,
};
use rc_core::domain::Room;
use rc_core::rooms::RoomChange;

use crate::hub;
use crate::room_view::kind_name;
use crate::row_ops::{RowOp, row_ops};

use qobject::RoomListModel;

/// `Qt::DisplayRole`, which shows the name too.
const DISPLAY_ROLE: i32 = 0;

// The custom roles, from `Qt::UserRole` on. Plain constants, as cxx-qt's `qenum` can't set values
// and the default ones would start at `Qt::DisplayRole`.
const ROOM_ID_ROLE: i32 = 256;
const NAME_ROLE: i32 = 257;
const KIND_ROLE: i32 = 258;
const UNREAD_ROLE: i32 = 259;
const USER_MENTIONS_ROLE: i32 = 260;
const GROUP_MENTIONS_ROLE: i32 = 261;
const ALERT_ROLE: i32 = 262;

/// Backing data of [`qobject::RoomListModel`]: the rooms, mirrored from the core's changes.
#[derive(Default)]
pub struct RoomListModelRust {
    rooms: Vec<Room>,
}

/// Counts are `u32`, Qt's are `int`: saturate.
fn count(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

/// `index` as the `int` Qt wants. Rooms never come near `i32::MAX`.
fn row(index: usize) -> i32 {
    i32::try_from(index).unwrap_or(i32::MAX)
}

impl cxx_qt::Initialize for RoomListModel {
    fn initialize(mut self: Pin<&mut Self>) {
        let snapshot = hub::register_rooms(self.qt_thread());
        self.as_mut().apply(&[snapshot]);
    }
}

impl RoomListModel {
    /// Applies the changes in order, announcing each one to the views.
    pub fn apply(mut self: Pin<&mut Self>, changes: &[RoomChange]) {
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
        match role {
            DISPLAY_ROLE | NAME_ROLE => QVariant::from(&QString::from(&room.name)),
            ROOM_ID_ROLE => QVariant::from(&QString::from(room.id.as_str())),
            KIND_ROLE => QVariant::from(&QString::from(kind_name(room.kind))),
            UNREAD_ROLE => QVariant::from(&count(room.unread)),
            USER_MENTIONS_ROLE => QVariant::from(&count(room.user_mentions)),
            GROUP_MENTIONS_ROLE => QVariant::from(&count(room.group_mentions)),
            ALERT_ROLE => QVariant::from(&room.alert),
            _ => QVariant::default(),
        }
    }
    #[allow(clippy::unused_self)] // Signature fixed by `QAbstractItemModel::roleNames`.
    fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut names = QHash::<QHashPair_i32_QByteArray>::default();
        for (role, name) in [
            (DISPLAY_ROLE, "display"),
            (ROOM_ID_ROLE, "roomId"),
            (NAME_ROLE, "name"),
            (KIND_ROLE, "kind"),
            (UNREAD_ROLE, "unread"),
            (USER_MENTIONS_ROLE, "userMentions"),
            (GROUP_MENTIONS_ROLE, "groupMentions"),
            (ALERT_ROLE, "alert"),
        ] {
            names.insert(role, QByteArray::from(name));
        }
        names
    }
}

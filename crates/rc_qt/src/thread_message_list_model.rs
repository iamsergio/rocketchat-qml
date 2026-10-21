// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The `ThreadMessageList` model: the open thread's messages, the parent and then the replies,
//! held and announced as `MessageList` does (newest first, for a view that runs bottom to top).
//! The parent joins when the page that reaches the start does, so older replies are plain
//! additions after the last row; opening another thread resets it.

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
        /// Available in QML as the `ThreadMessageList` singleton.
        #[qobject]
        #[qml_element = "ThreadMessageList"]
        #[qml_singleton]
        #[base = QAbstractListModel]
        type ThreadMessageListModel = super::ThreadMessageListModelRust;

        #[qsignal]
        #[inherit]
        #[cxx_name = "dataChanged"]
        fn data_changed(
            self: Pin<&mut ThreadMessageListModel>,
            top_left: &QModelIndex,
            bottom_right: &QModelIndex,
            roles: &QVector_i32,
        );

        #[inherit]
        fn index(
            self: &ThreadMessageListModel,
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
            self: Pin<&mut ThreadMessageListModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        /// # Safety
        ///
        /// Only after `begin_insert_rows`.
        #[inherit]
        #[cxx_name = "endInsertRows"]
        unsafe fn end_insert_rows(self: Pin<&mut ThreadMessageListModel>);

        /// # Safety
        ///
        /// Call `end_remove_rows` after changing the data.
        #[inherit]
        #[cxx_name = "beginRemoveRows"]
        unsafe fn begin_remove_rows(
            self: Pin<&mut ThreadMessageListModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        /// # Safety
        ///
        /// Only after `begin_remove_rows`.
        #[inherit]
        #[cxx_name = "endRemoveRows"]
        unsafe fn end_remove_rows(self: Pin<&mut ThreadMessageListModel>);

        /// Returns `false` if the move is invalid, in which case there is nothing to end.
        ///
        /// # Safety
        ///
        /// If it returns `true`, call `end_move_rows` after changing the data.
        #[inherit]
        #[cxx_name = "beginMoveRows"]
        unsafe fn begin_move_rows(
            self: Pin<&mut ThreadMessageListModel>,
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
        unsafe fn end_move_rows(self: Pin<&mut ThreadMessageListModel>);

        /// # Safety
        ///
        /// Call `end_reset_model` after changing the data.
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut ThreadMessageListModel>);
        /// # Safety
        ///
        /// Only after `begin_reset_model`.
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut ThreadMessageListModel>);
    }

    extern "RustQt" {
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &ThreadMessageListModel, parent: &QModelIndex) -> i32;

        #[cxx_override]
        fn data(self: &ThreadMessageListModel, index: &QModelIndex, role: i32) -> QVariant;

        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &ThreadMessageListModel) -> QHash_i32_QByteArray;
    }

    impl cxx_qt::Threading for ThreadMessageListModel {}
    impl cxx_qt::Initialize for ThreadMessageListModel {}
}

use core::pin::Pin;

use cxx_qt::{CxxQtType, Threading};

use crate::hub;
use crate::message_mirror::{MessageMirror, message_model};

use qobject::ThreadMessageListModel;

/// Backing data of [`qobject::ThreadMessageListModel`]: the open thread's messages, mirrored from
/// the core's changes and held newest first.
#[derive(Default)]
pub struct ThreadMessageListModelRust {
    mirror: MessageMirror,
}

impl cxx_qt::Initialize for ThreadMessageListModel {
    fn initialize(mut self: Pin<&mut Self>) {
        let (snapshot, viewer, avatars, images) = hub::register_thread_messages(self.qt_thread());
        self.as_mut().rust_mut().mirror = MessageMirror::new(viewer, avatars, images);
        self.as_mut().apply(&[snapshot]);
    }
}

message_model!(ThreadMessageListModel);

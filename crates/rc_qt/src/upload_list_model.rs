// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The `RoomUploads` and `ThreadUploads` models: the files being uploaded into the open room, or
//! into the open thread, one row each, in the order they were asked for. They are held and
//! announced from `rc_view::upload_view::changes`: a progress is a change of one row, and an upload
//! that is finished goes. Two singletons because two composers show them, and a list model that QML
//! creates as an element can't be linted (CXX-QT.md); `upload_model!` writes what they share.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++Qt" {
        include!(<QtCore/QAbstractListModel>);
        type QAbstractListModel = crate::room_list_model::qobject::QAbstractListModel;
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
        /// Available in QML as the `RoomUploads` singleton: the uploads into the open room.
        #[qobject]
        #[qml_element = "RoomUploads"]
        #[qml_singleton]
        #[base = QAbstractListModel]
        // `count`: how many rows there are, for the bar that lists them to show or hide.
        #[qproperty(i32, count, READ, NOTIFY)]
        type RoomUploadsModel = super::RoomUploadsModelRust;

        #[qsignal]
        #[inherit]
        #[cxx_name = "dataChanged"]
        fn data_changed(
            self: Pin<&mut RoomUploadsModel>,
            top_left: &QModelIndex,
            bottom_right: &QModelIndex,
            roles: &QVector_i32,
        );

        #[inherit]
        fn index(
            self: &RoomUploadsModel,
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
            self: Pin<&mut RoomUploadsModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        /// # Safety
        ///
        /// Only after `begin_insert_rows`.
        #[inherit]
        #[cxx_name = "endInsertRows"]
        unsafe fn end_insert_rows(self: Pin<&mut RoomUploadsModel>);

        /// # Safety
        ///
        /// Call `end_remove_rows` after changing the data.
        #[inherit]
        #[cxx_name = "beginRemoveRows"]
        unsafe fn begin_remove_rows(
            self: Pin<&mut RoomUploadsModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        /// # Safety
        ///
        /// Only after `begin_remove_rows`.
        #[inherit]
        #[cxx_name = "endRemoveRows"]
        unsafe fn end_remove_rows(self: Pin<&mut RoomUploadsModel>);

        /// Returns `false` if the move is invalid, in which case there is nothing to end.
        ///
        /// # Safety
        ///
        /// If it returns `true`, call `end_move_rows` after changing the data.
        #[inherit]
        #[cxx_name = "beginMoveRows"]
        unsafe fn begin_move_rows(
            self: Pin<&mut RoomUploadsModel>,
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
        unsafe fn end_move_rows(self: Pin<&mut RoomUploadsModel>);

        /// # Safety
        ///
        /// Call `end_reset_model` after changing the data.
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut RoomUploadsModel>);
        /// # Safety
        ///
        /// Only after `begin_reset_model`.
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut RoomUploadsModel>);
    }

    extern "RustQt" {
        /// Cancels the upload in the row: takes a queued or failed one out, stops one under way.
        #[qinvokable]
        fn cancel(self: Pin<&mut RoomUploadsModel>, row: i32);

        /// Puts the failed upload in the row back in the queue.
        #[qinvokable]
        fn retry(self: Pin<&mut RoomUploadsModel>, row: i32);
    }

    extern "RustQt" {
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &RoomUploadsModel, parent: &QModelIndex) -> i32;

        #[cxx_override]
        fn data(self: &RoomUploadsModel, index: &QModelIndex, role: i32) -> QVariant;

        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &RoomUploadsModel) -> QHash_i32_QByteArray;
    }

    impl cxx_qt::Threading for RoomUploadsModel {}
    impl cxx_qt::Initialize for RoomUploadsModel {}

    unsafe extern "RustQt" {
        /// Available in QML as the `ThreadUploads` singleton: the uploads into the open thread.
        #[qobject]
        #[qml_element = "ThreadUploads"]
        #[qml_singleton]
        #[base = QAbstractListModel]
        // `count`: how many rows there are, for the bar that lists them to show or hide.
        #[qproperty(i32, count, READ, NOTIFY)]
        type ThreadUploadsModel = super::ThreadUploadsModelRust;

        #[qsignal]
        #[inherit]
        #[cxx_name = "dataChanged"]
        fn data_changed(
            self: Pin<&mut ThreadUploadsModel>,
            top_left: &QModelIndex,
            bottom_right: &QModelIndex,
            roles: &QVector_i32,
        );

        #[inherit]
        fn index(
            self: &ThreadUploadsModel,
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
            self: Pin<&mut ThreadUploadsModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        /// # Safety
        ///
        /// Only after `begin_insert_rows`.
        #[inherit]
        #[cxx_name = "endInsertRows"]
        unsafe fn end_insert_rows(self: Pin<&mut ThreadUploadsModel>);

        /// # Safety
        ///
        /// Call `end_remove_rows` after changing the data.
        #[inherit]
        #[cxx_name = "beginRemoveRows"]
        unsafe fn begin_remove_rows(
            self: Pin<&mut ThreadUploadsModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        /// # Safety
        ///
        /// Only after `begin_remove_rows`.
        #[inherit]
        #[cxx_name = "endRemoveRows"]
        unsafe fn end_remove_rows(self: Pin<&mut ThreadUploadsModel>);

        /// Returns `false` if the move is invalid, in which case there is nothing to end.
        ///
        /// # Safety
        ///
        /// If it returns `true`, call `end_move_rows` after changing the data.
        #[inherit]
        #[cxx_name = "beginMoveRows"]
        unsafe fn begin_move_rows(
            self: Pin<&mut ThreadUploadsModel>,
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
        unsafe fn end_move_rows(self: Pin<&mut ThreadUploadsModel>);

        /// # Safety
        ///
        /// Call `end_reset_model` after changing the data.
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut ThreadUploadsModel>);
        /// # Safety
        ///
        /// Only after `begin_reset_model`.
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut ThreadUploadsModel>);
    }

    extern "RustQt" {
        /// Cancels the upload in the row: takes a queued or failed one out, stops one under way.
        #[qinvokable]
        fn cancel(self: Pin<&mut ThreadUploadsModel>, row: i32);

        /// Puts the failed upload in the row back in the queue.
        #[qinvokable]
        fn retry(self: Pin<&mut ThreadUploadsModel>, row: i32);
    }

    extern "RustQt" {
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &ThreadUploadsModel, parent: &QModelIndex) -> i32;

        #[cxx_override]
        fn data(self: &ThreadUploadsModel, index: &QModelIndex, role: i32) -> QVariant;

        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &ThreadUploadsModel) -> QHash_i32_QByteArray;
    }

    impl cxx_qt::Threading for ThreadUploadsModel {}
    impl cxx_qt::Initialize for ThreadUploadsModel {}
}

use core::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant, QVector};
use rc_view::row_ops::{RowOp, row_ops};
use rc_view::upload_view::{UploadRow, changes};

use crate::hub;
use crate::room_roles::row;

use qobject::{RoomUploadsModel, ThreadUploadsModel};

// The custom roles, from `Qt::UserRole` on (see `RoomListModel`).
const NAME_ROLE: i32 = 256;
const SIZE_ROLE: i32 = 257;
const STATUS_LINE_ROLE: i32 = 258;
const STATUS_ROLE: i32 = 259;
const PROGRESS_ROLE: i32 = 260;
const CAN_RETRY_ROLE: i32 = 261;
const CAN_CANCEL_ROLE: i32 = 262;
const IS_IMAGE_ROLE: i32 = 263;
const THUMBNAIL_ROLE: i32 = 264;
const LABEL_ROLE: i32 = 265;

/// The model of the uploads into the open room or thread, with the function that registers it
/// with the hub, and its backing data: the rows as the last change left them.
macro_rules! upload_model {
    ($model:ident, $rust:ident, $register:path) => {
        #[derive(Default)]
        pub struct $rust {
            rows: Vec<UploadRow>,
            count: i32,
        }

        impl cxx_qt::Initialize for $model {
            fn initialize(mut self: Pin<&mut Self>) {
                let rows = $register(self.qt_thread());
                self.as_mut().show(&rows);
            }
        }

        impl $model {
            fn cancel(self: Pin<&mut Self>, at: i32) {
                if let Some(upload) = self.upload_at(at) {
                    hub::cancel_upload(upload);
                }
            }

            fn retry(self: Pin<&mut Self>, at: i32) {
                if let Some(upload) = self.upload_at(at) {
                    hub::retry_upload(upload);
                }
            }

            fn upload_at(&self, at: i32) -> Option<rc_core::uploads::UploadId> {
                usize::try_from(at)
                    .ok()
                    .and_then(|at| self.rows.get(at))
                    .map(|row| row.id)
            }

            /// Brings the rows to `new` with the fewest changes: see [`changes`].
            pub fn show(mut self: Pin<&mut Self>, new: &[UploadRow]) {
                let all = changes(&self.rows, new);
                let no_parent = QModelIndex::default();
                for change in &all {
                    match row_ops(change) {
                        RowOp::Insert { row: at } => {
                            // SAFETY: balanced by `end_insert_rows` below.
                            unsafe {
                                self.as_mut()
                                    .begin_insert_rows(&no_parent, row(at), row(at));
                            }
                            change.apply(&mut self.as_mut().rust_mut().rows);
                            // SAFETY: after `begin_insert_rows`.
                            unsafe { self.as_mut().end_insert_rows() };
                        }
                        RowOp::Update { row: at } => {
                            change.apply(&mut self.as_mut().rust_mut().rows);
                            let index = self.index(row(at), 0, &no_parent);
                            // No roles listed means all of them.
                            self.as_mut()
                                .data_changed(&index, &index, &QVector::<i32>::default());
                        }
                        RowOp::Remove { row: at } => {
                            // SAFETY: balanced by `end_remove_rows` below.
                            unsafe {
                                self.as_mut()
                                    .begin_remove_rows(&no_parent, row(at), row(at));
                            }
                            change.apply(&mut self.as_mut().rust_mut().rows);
                            // SAFETY: after `begin_remove_rows`.
                            unsafe { self.as_mut().end_remove_rows() };
                        }
                        // `changes` makes none of these.
                        RowOp::Reset | RowOp::Move { .. } => {
                            // SAFETY: balanced by `end_reset_model` below.
                            unsafe { self.as_mut().begin_reset_model() };
                            change.apply(&mut self.as_mut().rust_mut().rows);
                            // SAFETY: after `begin_reset_model`.
                            unsafe { self.as_mut().end_reset_model() };
                        }
                    }
                }
                let count = row(self.rows.len());
                if self.count != count {
                    self.as_mut().rust_mut().count = count;
                    self.count_changed();
                }
            }

            fn row_count(&self, _parent: &QModelIndex) -> i32 {
                row(self.rows.len())
            }

            fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
                let Some(upload) = usize::try_from(index.row())
                    .ok()
                    .and_then(|at| self.rows.get(at))
                else {
                    return QVariant::default();
                };
                upload_role(upload, role)
            }

            #[allow(clippy::unused_self)] // Signature fixed by `QAbstractItemModel::roleNames`.
            fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
                upload_role_names()
            }
        }
    };
}

upload_model!(
    RoomUploadsModel,
    RoomUploadsModelRust,
    hub::register_room_uploads
);
upload_model!(
    ThreadUploadsModel,
    ThreadUploadsModelRust,
    hub::register_thread_uploads
);

/// What the row says in `role`.
fn upload_role(upload: &UploadRow, role: i32) -> QVariant {
    let text = |value: &str| QVariant::from(&QString::from(value));
    match role {
        NAME_ROLE => text(&upload.name),
        SIZE_ROLE => text(&upload.size),
        STATUS_LINE_ROLE => text(&upload.status_line),
        STATUS_ROLE => text(upload.status.name()),
        PROGRESS_ROLE => QVariant::from(&upload.progress),
        CAN_RETRY_ROLE => QVariant::from(&upload.can_retry),
        CAN_CANCEL_ROLE => QVariant::from(&upload.can_cancel),
        IS_IMAGE_ROLE => QVariant::from(&upload.thumbnail.is_image),
        THUMBNAIL_ROLE => text(&upload.thumbnail.url),
        LABEL_ROLE => text(&upload.thumbnail.label),
        _ => QVariant::default(),
    }
}

fn upload_role_names() -> QHash<QHashPair_i32_QByteArray> {
    let mut names = QHash::<QHashPair_i32_QByteArray>::default();
    for (role, name) in [
        (NAME_ROLE, "name"),
        (SIZE_ROLE, "size"),
        (STATUS_LINE_ROLE, "statusLine"),
        (STATUS_ROLE, "status"),
        (PROGRESS_ROLE, "progress"),
        (CAN_RETRY_ROLE, "canRetry"),
        (CAN_CANCEL_ROLE, "canCancel"),
        (IS_IMAGE_ROLE, "isImage"),
        (THUMBNAIL_ROLE, "thumbnail"),
        (LABEL_ROLE, "label"),
    ] {
        names.insert(role, cxx_qt_lib::QByteArray::from(name));
    }
    names
}

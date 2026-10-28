// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The `RoomStagedFiles` and `ThreadStagedFiles` models: the files attached to a composer that
//! wait for Send (ACT-009), in the order they were attached. They are the composer's own state:
//! `Composer.qml` clears them when its room or thread changes. Two singletons for the two
//! composers, as the uploads' (see `upload_list_model`); `staged_model!` writes what they share.

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
        /// Available in QML as the `RoomStagedFiles` singleton: the files attached to the
        /// room's composer.
        #[qobject]
        #[qml_element = "RoomStagedFiles"]
        #[qml_singleton]
        #[base = QAbstractListModel]
        // `count`: how many files wait, for the bar that lists them to show or hide, and for Send.
        #[qproperty(i32, count, READ, NOTIFY)]
        type RoomStagedFilesModel = super::RoomStagedFilesModelRust;

        #[qsignal]
        #[inherit]
        #[cxx_name = "dataChanged"]
        fn data_changed(
            self: Pin<&mut RoomStagedFilesModel>,
            top_left: &QModelIndex,
            bottom_right: &QModelIndex,
            roles: &QVector_i32,
        );

        #[inherit]
        fn index(
            self: &RoomStagedFilesModel,
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
            self: Pin<&mut RoomStagedFilesModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        /// # Safety
        ///
        /// Only after `begin_insert_rows`.
        #[inherit]
        #[cxx_name = "endInsertRows"]
        unsafe fn end_insert_rows(self: Pin<&mut RoomStagedFilesModel>);

        /// # Safety
        ///
        /// Call `end_remove_rows` after changing the data.
        #[inherit]
        #[cxx_name = "beginRemoveRows"]
        unsafe fn begin_remove_rows(
            self: Pin<&mut RoomStagedFilesModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        /// # Safety
        ///
        /// Only after `begin_remove_rows`.
        #[inherit]
        #[cxx_name = "endRemoveRows"]
        unsafe fn end_remove_rows(self: Pin<&mut RoomStagedFilesModel>);

        /// Returns `false` if the move is invalid, in which case there is nothing to end.
        ///
        /// # Safety
        ///
        /// If it returns `true`, call `end_move_rows` after changing the data.
        #[inherit]
        #[cxx_name = "beginMoveRows"]
        unsafe fn begin_move_rows(
            self: Pin<&mut RoomStagedFilesModel>,
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
        unsafe fn end_move_rows(self: Pin<&mut RoomStagedFilesModel>);

        /// # Safety
        ///
        /// Call `end_reset_model` after changing the data.
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut RoomStagedFilesModel>);
        /// # Safety
        ///
        /// Only after `begin_reset_model`.
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut RoomStagedFilesModel>);
    }

    extern "RustQt" {
        /// Adds the file the `file://` URL names, unless it waits already.
        #[qinvokable]
        fn add(self: Pin<&mut RoomStagedFilesModel>, url: &QString);

        /// Adds the files the test hook names (`RC_TEST_ATTACH_FILES`) and returns true, or
        /// returns false if it names none, so that the caller asks the user instead.
        #[qinvokable]
        #[cxx_name = "addTestFiles"]
        fn add_test_files(self: Pin<&mut RoomStagedFilesModel>) -> bool;

        /// Attaches what the clipboard holds, as `rc_view::paste` decides, and returns true, or
        /// returns false if the text field should paste instead.
        #[qinvokable]
        fn paste(self: Pin<&mut RoomStagedFilesModel>) -> bool;

        /// Takes the file in the row out. A pasted picture is deleted with it.
        #[qinvokable]
        fn remove(self: Pin<&mut RoomStagedFilesModel>, row: i32);

        /// Takes all the files out, deleting the pasted pictures among them.
        #[qinvokable]
        fn clear(self: Pin<&mut RoomStagedFilesModel>);

        /// Sends the text with the files to the open room, as `rc_view::staged_view::plan` says: the
        /// files are uploaded in order, the text with the first one. Quoting the message
        /// `quoted_id` (empty for none) makes the text a message of its own, sent first. The files
        /// leave the list. Returns whether anything was sent.
        #[qinvokable]
        fn submit(
            self: Pin<&mut RoomStagedFilesModel>,
            text: &QString,
            quoted_id: &QString,
            also_to_channel: bool,
        ) -> bool;
    }

    extern "RustQt" {
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &RoomStagedFilesModel, parent: &QModelIndex) -> i32;

        #[cxx_override]
        fn data(self: &RoomStagedFilesModel, index: &QModelIndex, role: i32) -> QVariant;

        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &RoomStagedFilesModel) -> QHash_i32_QByteArray;
    }

    impl cxx_qt::Threading for RoomStagedFilesModel {}
    impl cxx_qt::Initialize for RoomStagedFilesModel {}

    unsafe extern "RustQt" {
        /// Available in QML as the `ThreadStagedFiles` singleton: the files attached to the
        /// thread's composer.
        #[qobject]
        #[qml_element = "ThreadStagedFiles"]
        #[qml_singleton]
        #[base = QAbstractListModel]
        // `count`: how many files wait, for the bar that lists them to show or hide, and for Send.
        #[qproperty(i32, count, READ, NOTIFY)]
        type ThreadStagedFilesModel = super::ThreadStagedFilesModelRust;

        #[qsignal]
        #[inherit]
        #[cxx_name = "dataChanged"]
        fn data_changed(
            self: Pin<&mut ThreadStagedFilesModel>,
            top_left: &QModelIndex,
            bottom_right: &QModelIndex,
            roles: &QVector_i32,
        );

        #[inherit]
        fn index(
            self: &ThreadStagedFilesModel,
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
            self: Pin<&mut ThreadStagedFilesModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        /// # Safety
        ///
        /// Only after `begin_insert_rows`.
        #[inherit]
        #[cxx_name = "endInsertRows"]
        unsafe fn end_insert_rows(self: Pin<&mut ThreadStagedFilesModel>);

        /// # Safety
        ///
        /// Call `end_remove_rows` after changing the data.
        #[inherit]
        #[cxx_name = "beginRemoveRows"]
        unsafe fn begin_remove_rows(
            self: Pin<&mut ThreadStagedFilesModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        /// # Safety
        ///
        /// Only after `begin_remove_rows`.
        #[inherit]
        #[cxx_name = "endRemoveRows"]
        unsafe fn end_remove_rows(self: Pin<&mut ThreadStagedFilesModel>);

        /// Returns `false` if the move is invalid, in which case there is nothing to end.
        ///
        /// # Safety
        ///
        /// If it returns `true`, call `end_move_rows` after changing the data.
        #[inherit]
        #[cxx_name = "beginMoveRows"]
        unsafe fn begin_move_rows(
            self: Pin<&mut ThreadStagedFilesModel>,
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
        unsafe fn end_move_rows(self: Pin<&mut ThreadStagedFilesModel>);

        /// # Safety
        ///
        /// Call `end_reset_model` after changing the data.
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut ThreadStagedFilesModel>);
        /// # Safety
        ///
        /// Only after `begin_reset_model`.
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut ThreadStagedFilesModel>);
    }

    extern "RustQt" {
        /// Adds the file the `file://` URL names, unless it waits already.
        #[qinvokable]
        fn add(self: Pin<&mut ThreadStagedFilesModel>, url: &QString);

        /// Adds the files the test hook names (`RC_TEST_ATTACH_FILES`) and returns true, or
        /// returns false if it names none, so that the caller asks the user instead.
        #[qinvokable]
        #[cxx_name = "addTestFiles"]
        fn add_test_files(self: Pin<&mut ThreadStagedFilesModel>) -> bool;

        /// Attaches what the clipboard holds, as `rc_view::paste` decides, and returns true, or
        /// returns false if the text field should paste instead.
        #[qinvokable]
        fn paste(self: Pin<&mut ThreadStagedFilesModel>) -> bool;

        /// Takes the file in the row out. A pasted picture is deleted with it.
        #[qinvokable]
        fn remove(self: Pin<&mut ThreadStagedFilesModel>, row: i32);

        /// Takes all the files out, deleting the pasted pictures among them.
        #[qinvokable]
        fn clear(self: Pin<&mut ThreadStagedFilesModel>);

        /// Sends the text with the files to the open thread, as `rc_view::staged_view::plan` says: the
        /// files are uploaded in order, the text with the first one. Quoting the message
        /// `quoted_id` (empty for none) makes the text a message of its own, sent first. The files
        /// leave the list. Returns whether anything was sent.
        #[qinvokable]
        fn submit(
            self: Pin<&mut ThreadStagedFilesModel>,
            text: &QString,
            quoted_id: &QString,
            also_to_channel: bool,
        ) -> bool;
    }

    extern "RustQt" {
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &ThreadStagedFilesModel, parent: &QModelIndex) -> i32;

        #[cxx_override]
        fn data(self: &ThreadStagedFilesModel, index: &QModelIndex, role: i32) -> QVariant;

        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &ThreadStagedFilesModel) -> QHash_i32_QByteArray;
    }

    impl cxx_qt::Threading for ThreadStagedFilesModel {}
    impl cxx_qt::Initialize for ThreadStagedFilesModel {}
}

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant};
use rc_view::file_urls::file_path;
use rc_view::staged_view::{Staged, StagedFile};
use rc_view::startup::test_attachments;

use crate::hub;
use crate::room_roles::row;

use qobject::{RoomStagedFilesModel, ThreadStagedFilesModel};

// The custom roles, from `Qt::UserRole` on (see `RoomListModel`).
const NAME_ROLE: i32 = 256;
const SIZE_ROLE: i32 = 257;
const IS_IMAGE_ROLE: i32 = 258;
const THUMBNAIL_ROLE: i32 = 259;
const LABEL_ROLE: i32 = 260;

/// The model of the files waiting in the room's (`for_thread` false) or the thread's composer, and
/// its backing data.
macro_rules! staged_model {
    ($model:ident, $rust:ident, $for_thread:expr) => {
        #[derive(Default)]
        pub struct $rust {
            staged: Staged,
            count: i32,
        }

        impl cxx_qt::Initialize for $model {
            fn initialize(self: Pin<&mut Self>) {}
        }

        impl $model {
            fn add(self: Pin<&mut Self>, url: &QString) {
                if let Some(path) = file_path(&url.to_string()) {
                    self.add_paths(vec![path]);
                } else {
                    tracing::warn!(%url, "not a local file, not attached");
                }
            }

            fn add_test_files(self: Pin<&mut Self>) -> bool {
                let paths = test_attachments();
                if paths.is_empty() {
                    return false;
                }
                self.add_paths(paths);
                true
            }

            fn paste(self: Pin<&mut Self>) -> bool {
                let files = crate::clipboard::files_to_paste();
                if files.is_empty() {
                    return false;
                }
                self.add_paths(files);
                true
            }

            fn add_paths(mut self: Pin<&mut Self>, paths: Vec<std::path::PathBuf>) {
                let no_parent = QModelIndex::default();
                for path in paths {
                    if self.staged.has(&path) {
                        continue;
                    }
                    let at = self.staged.files().len();
                    // SAFETY: balanced by `end_insert_rows` below.
                    unsafe {
                        self.as_mut()
                            .begin_insert_rows(&no_parent, row(at), row(at));
                    }
                    self.as_mut().rust_mut().staged.add([path]);
                    // SAFETY: after `begin_insert_rows`.
                    unsafe { self.as_mut().end_insert_rows() };
                }
                self.count_now();
            }

            fn remove(mut self: Pin<&mut Self>, at: i32) {
                let Ok(index) = usize::try_from(at) else {
                    return;
                };
                if index >= self.staged.files().len() {
                    return;
                }
                let no_parent = QModelIndex::default();
                // SAFETY: balanced by `end_remove_rows` below.
                unsafe {
                    self.as_mut()
                        .begin_remove_rows(&no_parent, row(index), row(index));
                }
                let removed = self.as_mut().rust_mut().staged.remove(index);
                // SAFETY: after `begin_remove_rows`.
                unsafe { self.as_mut().end_remove_rows() };
                self.count_now();
                if let Some(file) = removed {
                    crate::clipboard::forget(&file.path);
                }
            }

            fn clear(self: Pin<&mut Self>) {
                for file in self.take_all() {
                    crate::clipboard::forget(&file.path);
                }
            }

            /// Takes all the files out and returns them.
            fn take_all(mut self: Pin<&mut Self>) -> Vec<StagedFile> {
                let all = self.staged.files().len();
                if all == 0 {
                    return Vec::new();
                }
                let no_parent = QModelIndex::default();
                // SAFETY: balanced by `end_remove_rows` below.
                unsafe {
                    self.as_mut()
                        .begin_remove_rows(&no_parent, 0, row(all - 1));
                }
                let files = self.as_mut().rust_mut().staged.take();
                // SAFETY: after `begin_remove_rows`.
                unsafe { self.as_mut().end_remove_rows() };
                self.count_now();
                files
            }

            fn submit(
                mut self: Pin<&mut Self>,
                text: &QString,
                quoted_id: &QString,
                also_to_channel: bool,
            ) -> bool {
                if self.staged.files().is_empty() {
                    return false;
                }
                let sent = hub::send_with_files(
                    $for_thread,
                    &text.to_string(),
                    &quoted_id.to_string(),
                    also_to_channel,
                    self.staged.files(),
                );
                if sent {
                    // The pasted files stay: the uploads read them from disk, and Retry needs them.
                    self.as_mut().take_all();
                }
                sent
            }

            /// Announces the count if it is not what it was.
            fn count_now(mut self: Pin<&mut Self>) {
                let count = row(self.staged.files().len());
                if self.count != count {
                    self.as_mut().rust_mut().count = count;
                    self.count_changed();
                }
            }

            fn row_count(&self, _parent: &QModelIndex) -> i32 {
                row(self.staged.files().len())
            }

            fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
                usize::try_from(index.row())
                    .ok()
                    .and_then(|at| self.staged.files().get(at))
                    .map_or_else(QVariant::default, |file| staged_role(file, role))
            }

            #[allow(clippy::unused_self)] // Signature fixed by `QAbstractItemModel::roleNames`.
            fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
                staged_role_names()
            }
        }
    };
}

staged_model!(RoomStagedFilesModel, RoomStagedFilesModelRust, false);
staged_model!(ThreadStagedFilesModel, ThreadStagedFilesModelRust, true);

/// What the file says in `role`.
fn staged_role(file: &StagedFile, role: i32) -> QVariant {
    let text = |value: &str| QVariant::from(&QString::from(value));
    match role {
        NAME_ROLE => text(&file.name),
        SIZE_ROLE => text(&file.size),
        IS_IMAGE_ROLE => QVariant::from(&file.thumbnail.is_image),
        THUMBNAIL_ROLE => text(&file.thumbnail.url),
        LABEL_ROLE => text(&file.thumbnail.label),
        _ => QVariant::default(),
    }
}

fn staged_role_names() -> QHash<QHashPair_i32_QByteArray> {
    let mut names = QHash::<QHashPair_i32_QByteArray>::default();
    for (role, name) in [
        (NAME_ROLE, "name"),
        (SIZE_ROLE, "size"),
        (IS_IMAGE_ROLE, "isImage"),
        (THUMBNAIL_ROLE, "thumbnail"),
        (LABEL_ROLE, "label"),
    ] {
        names.insert(role, cxx_qt_lib::QByteArray::from(name));
    }
    names
}

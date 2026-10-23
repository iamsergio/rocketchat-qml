// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The `RoomSearch` model: the rooms that match what the user typed in the search field of the
//! room list.

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
        /// Available in QML as the `RoomSearch` singleton.
        #[qobject]
        #[qml_element = "RoomSearch"]
        #[qml_singleton]
        #[base = QAbstractListModel]
        // Whether the query isn't empty: the results replace the room list then.
        #[qproperty(bool, active, READ, NOTIFY)]
        // The query without the whitespace around it, for the line that says nothing matches it.
        #[qproperty(QString, trimmed_query, cxx_name = "trimmedQuery", READ, NOTIFY)]
        type RoomSearchModel = super::RoomSearchModelRust;

        /// Searches for the rooms matching `query`. A new query is a new set of rows, so the model
        /// resets.
        #[qinvokable]
        #[cxx_name = "setQuery"]
        fn set_query(self: Pin<&mut RoomSearchModel>, query: &QString);

        /// Opens the room in the row. Returns whether there was one, so that the field knows to
        /// clear itself.
        #[qinvokable]
        fn open(self: &RoomSearchModel, row: i32) -> bool;

        #[qsignal]
        #[inherit]
        #[cxx_name = "dataChanged"]
        fn data_changed(
            self: Pin<&mut RoomSearchModel>,
            top_left: &QModelIndex,
            bottom_right: &QModelIndex,
            roles: &QVector_i32,
        );

        #[inherit]
        fn index(
            self: &RoomSearchModel,
            row: i32,
            column: i32,
            parent: &QModelIndex,
        ) -> QModelIndex;
    }

    // The begin/end pair must be kept balanced, so they are unsafe to call.
    extern "RustQt" {
        /// # Safety
        ///
        /// Call `end_reset_model` after changing the data.
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut RoomSearchModel>);
        /// # Safety
        ///
        /// Only after `begin_reset_model`.
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut RoomSearchModel>);
    }

    extern "RustQt" {
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &RoomSearchModel, parent: &QModelIndex) -> i32;

        #[cxx_override]
        fn data(self: &RoomSearchModel, index: &QModelIndex, role: i32) -> QVariant;

        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &RoomSearchModel) -> QHash_i32_QByteArray;
    }

    impl cxx_qt::Threading for RoomSearchModel {}
    impl cxx_qt::Initialize for RoomSearchModel {}
}

use core::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant, QVector};
use rc_core::domain::{AvatarOwner, Room};

use crate::hub;
use crate::room_roles::{AVATAR_ROLE, room_data, room_role_names, row};
use rc_view::avatar_view::{AvatarUrls, owners, rows_using};
use rc_view::room_search::trimmed;

use qobject::RoomSearchModel;

/// Backing data of [`qobject::RoomSearchModel`]: the query, and the rooms that match it, as the hub
/// found them.
#[derive(Default)]
pub struct RoomSearchModelRust {
    /// The avatars of the results, as they came with them and as they have changed since.
    avatars: AvatarUrls,
    query: String,
    results: Vec<Room>,
    active: bool,
    trimmed_query: QString,
}

impl cxx_qt::Initialize for RoomSearchModel {
    fn initialize(self: Pin<&mut Self>) {
        hub::register_room_search(self.qt_thread());
    }
}

impl RoomSearchModel {
    /// Searches again, as the rooms changed: another account's, or the same one's with a change.
    /// Nothing to do unless a search is going on.
    pub fn refresh(self: Pin<&mut Self>) {
        if self.active {
            self.search();
        }
    }

    /// Announces new content in the role of the avatar, and only that, for the rows that show
    /// the new files.
    pub fn set_avatars(mut self: Pin<&mut Self>, changes: &[(AvatarOwner, String)]) {
        self.as_mut().rust_mut().avatars.set(changes);
        let runs = rows_using(self.results.len(), &owners(changes), |at| {
            self.results.get(at).map(Room::avatar_owner)
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

    /// Replaces the results with the rooms that match the query now. A new result set, not a change
    /// of the old one, so the model resets.
    fn search(mut self: Pin<&mut Self>) {
        // SAFETY: balanced by `end_reset_model` below.
        unsafe { self.as_mut().begin_reset_model() };
        let (results, avatars) = hub::search_rooms(&self.query);
        self.as_mut().rust_mut().results = results;
        self.as_mut().rust_mut().avatars = avatars;
        // SAFETY: after `begin_reset_model`.
        unsafe { self.as_mut().end_reset_model() };
    }

    fn set_query(mut self: Pin<&mut Self>, query: &QString) {
        let query = query.to_string();
        let trimmed_query = QString::from(trimmed(&query));
        let active = !trimmed_query.is_empty();
        self.as_mut().rust_mut().query = query;
        self.as_mut().search();
        if self.trimmed_query != trimmed_query {
            self.as_mut().rust_mut().trimmed_query = trimmed_query;
            self.as_mut().trimmed_query_changed();
        }
        if self.active != active {
            self.as_mut().rust_mut().active = active;
            self.active_changed();
        }
    }

    #[allow(clippy::unused_self)] // The invokable needs an object; the core is process-wide.
    fn open(&self, row: i32) -> bool {
        let Some(room) = usize::try_from(row)
            .ok()
            .and_then(|at| self.results.get(at))
        else {
            return false;
        };
        hub::open_room(room.id.to_string());
        true
    }

    fn row_count(&self, _parent: &QModelIndex) -> i32 {
        row(self.results.len())
    }

    fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(room) = usize::try_from(index.row())
            .ok()
            .and_then(|at| self.results.get(at))
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

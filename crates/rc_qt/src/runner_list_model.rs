// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The `RunnerList` model: the self-hosted runners at the foot of the GitHub page, one row each.
//! There are a few of them and they change as a whole, so a change is a model reset.

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
    }

    unsafe extern "RustQt" {
        /// Available in QML as the `RunnerList` singleton.
        #[qobject]
        #[qml_element = "RunnerList"]
        #[qml_singleton]
        #[base = QAbstractListModel]
        type RunnerList = super::RunnerListRust;
    }

    // The begin/end pairs must be kept balanced, so they are unsafe to call.
    extern "RustQt" {
        /// # Safety
        ///
        /// Call `end_reset_model` after changing the data.
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut RunnerList>);
        /// # Safety
        ///
        /// Only after `begin_reset_model`.
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut RunnerList>);
    }

    extern "RustQt" {
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &RunnerList, parent: &QModelIndex) -> i32;

        #[cxx_override]
        fn data(self: &RunnerList, index: &QModelIndex, role: i32) -> QVariant;

        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &RunnerList) -> QHash_i32_QByteArray;
    }

    impl cxx_qt::Threading for RunnerList {}
    impl cxx_qt::Initialize for RunnerList {}
}

use core::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant};
use rc_view::github::{RunnerLine, RunnerState};

use crate::hub;
use crate::room_roles::row;

use qobject::RunnerList;

// The custom roles, from `Qt::UserRole` on.
const NAME_ROLE: i32 = 256;
const STATUS_ROLE: i32 = 257;
const STATE_ROLE: i32 = 258;

/// Backing data of [`qobject::RunnerList`].
#[derive(Default)]
pub struct RunnerListRust {
    lines: Vec<RunnerLine>,
}

impl cxx_qt::Initialize for RunnerList {
    fn initialize(mut self: Pin<&mut Self>) {
        let lines = hub::register_runners(self.qt_thread());
        self.as_mut().rust_mut().lines = lines;
    }
}

impl RunnerList {
    /// Shows these runners instead of the ones held.
    pub fn show(mut self: Pin<&mut Self>, lines: Vec<RunnerLine>) {
        if self.lines == lines {
            return;
        }
        // SAFETY: balanced by `end_reset_model` below.
        unsafe { self.as_mut().begin_reset_model() };
        self.as_mut().rust_mut().lines = lines;
        // SAFETY: after `begin_reset_model`.
        unsafe { self.as_mut().end_reset_model() };
    }

    fn row_count(&self, _parent: &QModelIndex) -> i32 {
        row(self.lines.len())
    }

    fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(line) = usize::try_from(index.row())
            .ok()
            .and_then(|at| self.lines.get(at))
        else {
            return QVariant::default();
        };
        let text = |value: &str| QVariant::from(&QString::from(value));
        match role {
            NAME_ROLE => text(&line.name),
            STATUS_ROLE => text(&line.status),
            STATE_ROLE => text(match line.state {
                RunnerState::Busy => "busy",
                RunnerState::Idle => "idle",
                RunnerState::Offline => "offline",
            }),
            _ => QVariant::default(),
        }
    }

    #[allow(clippy::unused_self)] // Signature fixed by `QAbstractItemModel::roleNames`.
    fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut names = QHash::<QHashPair_i32_QByteArray>::default();
        for (role, name) in [
            (NAME_ROLE, "name"),
            (STATUS_ROLE, "status"),
            (STATE_ROLE, "runnerState"),
        ] {
            names.insert(role, QByteArray::from(name));
        }
        names
    }
}

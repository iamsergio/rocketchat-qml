// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The `JobList` model: the jobs of the open pull request's checks, a heading per group
//! (Failed, Running, Cancelled, Passed) and under it the group's jobs while it is open. The
//! groups' folds are the user's to change (`toggleGroup`, `reveal`) and are forgotten when
//! another pull request opens. A poll that changes what the rows say is a `dataChanged`; only a
//! group coming or going, or another pull request, resets the model.

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
        /// Available in QML as the `JobList` singleton.
        #[qobject]
        #[qml_element = "JobList"]
        #[qml_singleton]
        #[base = QAbstractListModel]
        type JobList = super::JobListRust;

        /// Opens the group (`failed`, `running`, `cancelled` or `passed`) if it is folded, folds
        /// it if it is open.
        #[qinvokable]
        #[cxx_name = "toggleGroup"]
        fn toggle_group(self: Pin<&mut JobList>, group: &QString);

        /// Opens the group of the job at this position of the check bar if it is folded, and
        /// returns the row of the job, or -1 if there is none.
        #[qinvokable]
        fn reveal(self: Pin<&mut JobList>, job: i32) -> i32;

        #[qsignal]
        #[inherit]
        #[cxx_name = "dataChanged"]
        fn data_changed(
            self: Pin<&mut JobList>,
            top_left: &QModelIndex,
            bottom_right: &QModelIndex,
            roles: &QVector_i32,
        );

        #[inherit]
        fn index(self: &JobList, row: i32, column: i32, parent: &QModelIndex) -> QModelIndex;
    }

    // The begin/end pairs must be kept balanced, so they are unsafe to call.
    extern "RustQt" {
        /// # Safety
        ///
        /// Call `end_insert_rows` after changing the data.
        #[inherit]
        #[cxx_name = "beginInsertRows"]
        unsafe fn begin_insert_rows(
            self: Pin<&mut JobList>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        /// # Safety
        ///
        /// Only after `begin_insert_rows`.
        #[inherit]
        #[cxx_name = "endInsertRows"]
        unsafe fn end_insert_rows(self: Pin<&mut JobList>);

        /// # Safety
        ///
        /// Call `end_remove_rows` after changing the data.
        #[inherit]
        #[cxx_name = "beginRemoveRows"]
        unsafe fn begin_remove_rows(
            self: Pin<&mut JobList>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        /// # Safety
        ///
        /// Only after `begin_remove_rows`.
        #[inherit]
        #[cxx_name = "endRemoveRows"]
        unsafe fn end_remove_rows(self: Pin<&mut JobList>);

        /// # Safety
        ///
        /// Call `end_reset_model` after changing the data.
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut JobList>);
        /// # Safety
        ///
        /// Only after `begin_reset_model`.
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut JobList>);
    }

    extern "RustQt" {
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &JobList, parent: &QModelIndex) -> i32;

        #[cxx_override]
        fn data(self: &JobList, index: &QModelIndex, role: i32) -> QVariant;

        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &JobList) -> QHash_i32_QByteArray;
    }

    impl cxx_qt::Threading for JobList {}
    impl cxx_qt::Initialize for JobList {}
}

use core::pin::Pin;
use std::collections::HashMap;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{
    QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant, QVector,
};
use rc_view::github::JobGroupView;

use crate::hub;
use crate::job_rows::{
    JobRow, group_is_open, group_name, group_of_job, group_position, rows, state_name, state_of,
};
use crate::room_roles::row;

use qobject::JobList;

// The custom roles, from `Qt::UserRole` on.
const ROW_TYPE_ROLE: i32 = 256;
const GROUP_NAME_ROLE: i32 = 257;
const TITLE_ROLE: i32 = 258;
const OPEN_ROLE: i32 = 259;
const JOB_INDEX_ROLE: i32 = 260;
const STATE_ROLE: i32 = 261;
const WORKFLOW_ROLE: i32 = 262;
const NAME_ROLE: i32 = 263;
const DURATION_ROLE: i32 = 264;
const RERUN_VISIBLE_ROLE: i32 = 265;
const RERUN_ENABLED_ROLE: i32 = 266;
const RERUN_BUSY_ROLE: i32 = 267;
const URL_ROLE: i32 = 268;

/// Backing data of [`qobject::JobList`]: the groups of the open pull request, what the user
/// opened or folded, and the rows that follow.
#[derive(Default)]
pub struct JobListRust {
    /// The open pull request; the folds are its.
    key: String,
    groups: Vec<JobGroupView>,
    chosen: HashMap<&'static str, bool>,
    rows: Vec<JobRow>,
}

impl cxx_qt::Initialize for JobList {
    fn initialize(mut self: Pin<&mut Self>) {
        let jobs = hub::register_jobs(self.qt_thread());
        let mut this = self.as_mut().rust_mut();
        if let Some((key, groups)) = jobs {
            this.rows = rows(&groups, &this.chosen);
            this.key = key;
            this.groups = groups;
        }
    }
}

impl JobList {
    /// Shows these groups of the pull request `key`, or none (`None`) when no pull request is
    /// open.
    pub fn show(mut self: Pin<&mut Self>, jobs: Option<(String, Vec<JobGroupView>)>) {
        let (key, groups) = jobs.unwrap_or_default();
        let other = key != self.key;
        if other {
            self.as_mut().rust_mut().chosen.clear();
        }
        let new = rows(&groups, &self.chosen);
        self.as_mut().rust_mut().key = key;
        self.as_mut().rust_mut().groups = groups;
        let same_shape = !other
            && new.len() == self.rows.len()
            && new.iter().zip(&self.rows).all(|(a, b)| a.same_item(b));
        if same_shape {
            // The first and last rows that say something else.
            let changed: Vec<usize> = (0..new.len())
                .filter(|at| new[*at] != self.rows[*at])
                .collect();
            self.as_mut().rust_mut().rows = new;
            if let (Some(first), Some(last)) = (changed.first(), changed.last()) {
                self.as_mut().rows_changed(*first, *last);
            }
        } else if new != self.rows {
            // SAFETY: balanced by `end_reset_model` below.
            unsafe { self.as_mut().begin_reset_model() };
            self.as_mut().rust_mut().rows = new;
            // SAFETY: after `begin_reset_model`.
            unsafe { self.as_mut().end_reset_model() };
        }
    }

    /// Announces new content in every role of the rows `first` to `last`.
    fn rows_changed(mut self: Pin<&mut Self>, first: usize, last: usize) {
        let no_parent = QModelIndex::default();
        let top = self.index(row(first), 0, &no_parent);
        let bottom = self.index(row(last), 0, &no_parent);
        // No roles listed means all of them.
        self.as_mut()
            .data_changed(&top, &bottom, &QVector::<i32>::default());
    }

    /// Opens or folds the group `state`, announcing its jobs coming or going.
    fn set_group_open(mut self: Pin<&mut Self>, name: &'static str, open: bool) {
        let Some(state) = state_of(name) else {
            return;
        };
        if group_is_open(&self.groups, &self.chosen, state) == open {
            return;
        }
        let Some((at, jobs)) = group_position(&self.groups, &self.chosen, state) else {
            return;
        };
        self.as_mut().rust_mut().chosen.insert(name, open);
        let new = rows(&self.groups, &self.chosen);
        let no_parent = QModelIndex::default();
        if jobs > 0 {
            let (first, last) = (row(at + 1), row(at + jobs));
            if open {
                // SAFETY: balanced by `end_insert_rows` below.
                unsafe { self.as_mut().begin_insert_rows(&no_parent, first, last) };
                self.as_mut().rust_mut().rows = new;
                // SAFETY: after `begin_insert_rows`.
                unsafe { self.as_mut().end_insert_rows() };
            } else {
                // SAFETY: balanced by `end_remove_rows` below.
                unsafe { self.as_mut().begin_remove_rows(&no_parent, first, last) };
                self.as_mut().rust_mut().rows = new;
                // SAFETY: after `begin_remove_rows`.
                unsafe { self.as_mut().end_remove_rows() };
            }
        } else {
            self.as_mut().rust_mut().rows = new;
        }
        self.rows_changed(at, at);
    }

    fn toggle_group(self: Pin<&mut Self>, group: &QString) {
        let group = group.to_string();
        let Some(state) = state_of(&group) else {
            return;
        };
        let open = group_is_open(&self.groups, &self.chosen, state);
        self.set_group_open(state_name(state), !open);
    }

    fn reveal(mut self: Pin<&mut Self>, job: i32) -> i32 {
        let Some(state) = usize::try_from(job)
            .ok()
            .and_then(|index| group_of_job(&self.groups, index))
        else {
            return -1;
        };
        self.as_mut().set_group_open(state_name(state), true);
        let at = self.rows.iter().position(
            |shown| matches!(shown, JobRow::Job(shown) if i32::try_from(shown.index) == Ok(job)),
        );
        at.map_or(-1, row)
    }

    fn row_count(&self, _parent: &QModelIndex) -> i32 {
        row(self.rows.len())
    }

    fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(shown) = usize::try_from(index.row())
            .ok()
            .and_then(|at| self.rows.get(at))
        else {
            return QVariant::default();
        };
        let text = |value: &str| QVariant::from(&QString::from(value));
        match shown {
            JobRow::Group { state, title, open } => match role {
                ROW_TYPE_ROLE => text("group"),
                GROUP_NAME_ROLE => text(group_name(*state)),
                STATE_ROLE => text(state_name(*state)),
                TITLE_ROLE => text(title),
                OPEN_ROLE => QVariant::from(open),
                _ => QVariant::default(),
            },
            JobRow::Job(job) => match role {
                ROW_TYPE_ROLE => text("job"),
                JOB_INDEX_ROLE => QVariant::from(&row(job.index)),
                STATE_ROLE => text(state_name(job.state)),
                WORKFLOW_ROLE => text(&job.workflow_prefix),
                NAME_ROLE => text(&job.name),
                DURATION_ROLE => text(&job.duration),
                RERUN_VISIBLE_ROLE => QVariant::from(&job.rerun.visible),
                RERUN_ENABLED_ROLE => QVariant::from(&job.rerun.enabled),
                RERUN_BUSY_ROLE => QVariant::from(&job.rerun.busy),
                URL_ROLE => text(job.url.as_deref().unwrap_or_default()),
                _ => QVariant::default(),
            },
        }
    }

    #[allow(clippy::unused_self)] // Signature fixed by `QAbstractItemModel::roleNames`.
    fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut names = QHash::<QHashPair_i32_QByteArray>::default();
        for (role, name) in [
            (ROW_TYPE_ROLE, "rowType"),
            (GROUP_NAME_ROLE, "groupName"),
            (TITLE_ROLE, "groupTitle"),
            (OPEN_ROLE, "groupOpen"),
            (JOB_INDEX_ROLE, "jobIndex"),
            (STATE_ROLE, "jobState"),
            (WORKFLOW_ROLE, "workflow"),
            (NAME_ROLE, "jobName"),
            (DURATION_ROLE, "duration"),
            (RERUN_VISIBLE_ROLE, "rerunVisible"),
            (RERUN_ENABLED_ROLE, "rerunEnabled"),
            (RERUN_BUSY_ROLE, "rerunBusy"),
            (URL_ROLE, "url"),
        ] {
            names.insert(role, QByteArray::from(name));
        }
        names
    }
}

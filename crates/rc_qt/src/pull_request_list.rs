// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The `PullRequestList` model: the rows of one tab of the GitHub page (Mine or Watched), changed
//! one row at a time as the service's list changes. Its rows are the pull requests that are not
//! drafts, then a heading "Drafts n" and, while that is open, the drafts (`rc_view::github::fold`
//! plans the notifications). Only one tab shows at a time, so the model holds that one and the hub
//! gives it the other's rows when the page switches.

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
        /// Available in QML as the `PullRequestList` singleton.
        #[qobject]
        #[qml_element = "PullRequestList"]
        #[qml_singleton]
        #[base = QAbstractListModel]
        // The list shown: 0 is Mine, 1 is Watched.
        #[qproperty(i32, tab, READ, NOTIFY)]
        // The drafts show under their heading.
        #[qproperty(bool, drafts_open, cxx_name = "draftsOpen", READ, NOTIFY)]
        type PullRequestList = super::PullRequestListRust;

        /// Shows the other list, 0 for Mine and 1 for Watched. Drafts start folded.
        #[qinvokable]
        #[cxx_name = "showTab"]
        fn show_tab(self: Pin<&mut PullRequestList>, tab: i32);

        /// Opens the drafts under their heading, or folds them.
        #[qinvokable]
        #[cxx_name = "toggleDrafts"]
        fn toggle_drafts(self: Pin<&mut PullRequestList>);

        /// Opens the pull request of the row: the page shows it, and its row stops being bold.
        #[qinvokable]
        fn select(self: Pin<&mut PullRequestList>, row: i32);

        #[qsignal]
        #[inherit]
        #[cxx_name = "dataChanged"]
        fn data_changed(
            self: Pin<&mut PullRequestList>,
            top_left: &QModelIndex,
            bottom_right: &QModelIndex,
            roles: &QVector_i32,
        );

        #[inherit]
        fn index(
            self: &PullRequestList,
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
            self: Pin<&mut PullRequestList>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        /// # Safety
        ///
        /// Only after `begin_insert_rows`.
        #[inherit]
        #[cxx_name = "endInsertRows"]
        unsafe fn end_insert_rows(self: Pin<&mut PullRequestList>);

        /// # Safety
        ///
        /// Call `end_remove_rows` after changing the data.
        #[inherit]
        #[cxx_name = "beginRemoveRows"]
        unsafe fn begin_remove_rows(
            self: Pin<&mut PullRequestList>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        /// # Safety
        ///
        /// Only after `begin_remove_rows`.
        #[inherit]
        #[cxx_name = "endRemoveRows"]
        unsafe fn end_remove_rows(self: Pin<&mut PullRequestList>);

        /// Returns `false` if the move is invalid, in which case there is nothing to end.
        ///
        /// # Safety
        ///
        /// If it returns `true`, call `end_move_rows` after changing the data.
        #[inherit]
        #[cxx_name = "beginMoveRows"]
        unsafe fn begin_move_rows(
            self: Pin<&mut PullRequestList>,
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
        unsafe fn end_move_rows(self: Pin<&mut PullRequestList>);

        /// # Safety
        ///
        /// Call `end_reset_model` after changing the data.
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut PullRequestList>);
        /// # Safety
        ///
        /// Only after `begin_reset_model`.
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut PullRequestList>);
    }

    extern "RustQt" {
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &PullRequestList, parent: &QModelIndex) -> i32;

        #[cxx_override]
        fn data(self: &PullRequestList, index: &QModelIndex, role: i32) -> QVariant;

        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &PullRequestList) -> QHash_i32_QByteArray;
    }

    impl cxx_qt::Threading for PullRequestList {}
    impl cxx_qt::Initialize for PullRequestList {}
}

use core::pin::Pin;
use std::collections::HashSet;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{
    QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QStringList, QVariant,
    QVector,
};
use jiff::Timestamp;
use rc_core::github::PrKey;
use rc_core::list_change::ListChange;
use rc_view::github::Tab;
use rc_view::github::fold::{Around, Edit, Fold, Row, Step};

use crate::github_view::{Entry, fold_change, glyph_name, segment_name};
use crate::hub;
use crate::room_roles::row;

use qobject::PullRequestList;

// The custom roles, from `Qt::UserRole` on.
const ROW_TYPE_ROLE: i32 = 256;
const PR_ID_ROLE: i32 = 257;
const GLYPH_ROLE: i32 = 258;
const REPO_LINE_ROLE: i32 = 259;
/// The only role that changes when the minute turns.
const AGE_ROLE: i32 = 260;
const BELL_ROLE: i32 = 261;
const TITLE_ROLE: i32 = 262;
const NUMBER_ROLE: i32 = 263;
const BAR_ROLE: i32 = 264;
const CHANGED_ROLE: i32 = 265;
const DRAFTS_TITLE_ROLE: i32 = 266;
const DRAFTS_OPEN_ROLE: i32 = 267;

/// What the hub gives a model that registers or switches tab: the rows, and the generation of
/// the list they are from. A change that is for an older one is stale: the rows have it already.
pub struct Snapshot {
    pub generation: u64,
    pub tab: Tab,
    pub entries: Vec<Entry>,
}

/// Backing data of [`qobject::PullRequestList`]: the service's list as rows, and how it folds.
#[derive(Default)]
pub struct PullRequestListRust {
    entries: Vec<Entry>,
    fold: Fold,
    generation: u64,
    tab: i32,
    drafts_open: bool,
}

const fn tab_number(tab: Tab) -> i32 {
    match tab {
        Tab::Mine => 0,
        Tab::Watched => 1,
    }
}

/// The item an insert, update or move brings.
const fn item_of(change: &ListChange<Entry>) -> Option<&Entry> {
    match change {
        ListChange::Inserted { item, .. }
        | ListChange::Updated { item, .. }
        | ListChange::Moved { item, .. } => Some(item),
        _ => None,
    }
}

impl cxx_qt::Initialize for PullRequestList {
    fn initialize(mut self: Pin<&mut Self>) {
        let snapshot = hub::register_pull_requests(self.qt_thread());
        // Nothing is looking at the model yet, so there is nothing to announce.
        let mut fold = Fold::default();
        fold.plan(fold_change(&ListChange::Reset(snapshot.entries.clone())));
        let mut rust = self.as_mut().rust_mut();
        rust.tab = tab_number(snapshot.tab);
        rust.generation = snapshot.generation;
        rust.entries = snapshot.entries;
        rust.fold = fold;
    }
}

impl PullRequestList {
    /// Shows the rows of `snapshot` in place of the ones held, as one model reset, with the drafts
    /// folded.
    pub fn show(mut self: Pin<&mut Self>, snapshot: Snapshot) {
        self.as_mut().rust_mut().generation = snapshot.generation;
        let tab = tab_number(snapshot.tab);
        let tab_changed = self.tab != tab;
        self.as_mut().rust_mut().tab = tab;
        let change = ListChange::Reset(snapshot.entries);
        let mut fold = Fold::default();
        let steps = fold.plan(fold_change(&change));
        self.as_mut().carry_out(&steps, &change);
        self.as_mut().set_drafts_open_flag(false);
        if tab_changed {
            self.tab_changed();
        }
    }

    /// Applies the changes the service made to its list. Changes made against an older list than
    /// the one shown are dropped: the snapshot has them.
    pub fn apply(mut self: Pin<&mut Self>, generation: u64, changes: &[ListChange<Entry>]) {
        if generation != self.generation {
            return;
        }
        for change in changes {
            let mut fold = self.fold;
            let steps = fold.plan(fold_change(change));
            self.as_mut().carry_out(&steps, change);
        }
    }

    /// Shows the age of every row as of `now`, announcing the role of the rows where it changed.
    pub fn refresh_ages(mut self: Pin<&mut Self>, now: Timestamp) {
        let mut changed = Vec::new();
        for (at, entry) in self.as_mut().rust_mut().entries.iter_mut().enumerate() {
            if entry.refresh_age(now) {
                changed.push(at);
            }
        }
        for at in changed {
            self.as_mut().role_changed(at, AGE_ROLE);
        }
    }

    /// Rings the bell of the rows whose pull request is in `notifying`, and only theirs.
    pub fn set_bells(mut self: Pin<&mut Self>, notifying: &HashSet<PrKey>) {
        let mut changed = Vec::new();
        for (at, entry) in self.as_mut().rust_mut().entries.iter_mut().enumerate() {
            let bell = notifying.contains(&entry.view.key);
            if entry.view.bell != bell {
                entry.view.bell = bell;
                changed.push(at);
            }
        }
        for at in changed {
            self.as_mut().role_changed(at, BELL_ROLE);
        }
    }

    /// Does what the steps say, in order. `change` brings the item of an insert, update or move,
    /// or the rows of a reset.
    fn carry_out(mut self: Pin<&mut Self>, steps: &[Step], change: &ListChange<Entry>) {
        let no_parent = QModelIndex::default();
        for step in steps {
            let mut moving = false;
            match step.around {
                Some(Around::Reset) => {
                    // SAFETY: balanced by `end_reset_model` below.
                    unsafe { self.as_mut().begin_reset_model() };
                }
                Some(Around::Insert { first, last }) => {
                    // SAFETY: balanced by `end_insert_rows` below.
                    unsafe {
                        self.as_mut()
                            .begin_insert_rows(&no_parent, row(first), row(last));
                    }
                }
                Some(Around::Remove { first, last }) => {
                    // SAFETY: balanced by `end_remove_rows` below.
                    unsafe {
                        self.as_mut()
                            .begin_remove_rows(&no_parent, row(first), row(last));
                    }
                }
                Some(Around::Move { from, destination }) => {
                    // SAFETY: `end_move_rows` is called below if this returns `true`.
                    moving = unsafe {
                        self.as_mut().begin_move_rows(
                            &no_parent,
                            row(from),
                            row(from),
                            &no_parent,
                            row(destination),
                        )
                    };
                }
                None => {}
            }
            self.as_mut().edit(step.edit, change);
            self.as_mut().rust_mut().fold = step.after;
            match step.around {
                // SAFETY: after the `begin_reset_model` above.
                Some(Around::Reset) => unsafe { self.as_mut().end_reset_model() },
                // SAFETY: after the `begin_insert_rows` above.
                Some(Around::Insert { .. }) => unsafe { self.as_mut().end_insert_rows() },
                // SAFETY: after the `begin_remove_rows` above.
                Some(Around::Remove { .. }) => unsafe { self.as_mut().end_remove_rows() },
                // SAFETY: after `begin_move_rows` returned `true`.
                Some(Around::Move { .. }) if moving => unsafe { self.as_mut().end_move_rows() },
                Some(Around::Move { .. }) | None => {}
            }
            if let Some(at) = step.changed {
                self.as_mut().row_changed(at);
            }
        }
    }

    /// Changes the rows held, as the service's list changed.
    fn edit(mut self: Pin<&mut Self>, edit: Edit, change: &ListChange<Entry>) {
        let mut rust = self.as_mut().rust_mut();
        let entries = &mut rust.entries;
        let item = item_of(change);
        match edit {
            Edit::Reset => {
                if let ListChange::Reset(all) = change {
                    entries.clone_from(all);
                }
            }
            Edit::Insert { index } => match item {
                Some(item) if index <= entries.len() => entries.insert(index, item.clone()),
                _ => tracing::error!(index, "a pull request row can't be inserted"),
            },
            Edit::Remove { index } => {
                if index < entries.len() {
                    entries.remove(index);
                } else {
                    tracing::error!(index, "a pull request row can't be removed");
                }
            }
            Edit::Set { index } => {
                if let (Some(item), Some(slot)) = (item, entries.get_mut(index)) {
                    slot.clone_from(item);
                } else {
                    tracing::error!(index, "a pull request row can't be replaced");
                }
            }
            Edit::Move { from, to } => match item {
                Some(item) if from < entries.len() && to < entries.len() => {
                    entries.remove(from);
                    entries.insert(to, item.clone());
                }
                _ => tracing::error!(from, to, "a pull request row can't be moved"),
            },
            Edit::None => {}
        }
    }

    /// Announces new content in every role of the row.
    fn row_changed(mut self: Pin<&mut Self>, at: usize) {
        let index = self.index(row(at), 0, &QModelIndex::default());
        // No roles listed means all of them.
        self.as_mut()
            .data_changed(&index, &index, &QVector::<i32>::default());
    }

    /// Announces new content in one role of the pull request at `at` of the service's list.
    fn role_changed(mut self: Pin<&mut Self>, at: usize, role: i32) {
        let Some(shown) = self.fold.row_of(at) else {
            return;
        };
        let index = self.index(row(shown), 0, &QModelIndex::default());
        let mut roles = QVector::<i32>::default();
        roles.append(role);
        self.as_mut().data_changed(&index, &index, &roles);
    }

    fn set_drafts_open_flag(mut self: Pin<&mut Self>, open: bool) {
        if self.drafts_open != open {
            self.as_mut().rust_mut().drafts_open = open;
            self.drafts_open_changed();
        }
    }

    fn show_tab(self: Pin<&mut Self>, tab: i32) {
        let tab = if tab == 1 { Tab::Watched } else { Tab::Mine };
        let snapshot = hub::github_show_tab(tab);
        self.show(snapshot);
    }

    fn toggle_drafts(mut self: Pin<&mut Self>) {
        let mut fold = self.fold;
        let open = !fold.open;
        let steps = fold.set_open(open);
        self.as_mut()
            .carry_out(&steps, &ListChange::Reset(Vec::new()));
        self.set_drafts_open_flag(open);
    }

    fn select(mut self: Pin<&mut Self>, shown: i32) {
        let Ok(shown) = usize::try_from(shown) else {
            return;
        };
        if shown >= self.fold.len() {
            return;
        }
        let Row::Pull(at) = self.fold.row(shown) else {
            return;
        };
        let mut rust = self.as_mut().rust_mut();
        let Some(entry) = rust.entries.get_mut(at) else {
            return;
        };
        let key = entry.view.key.clone();
        // The cache forgets what changed; the row says so at once.
        let was_bold = std::mem::take(&mut entry.view.changed);
        hub::github_select(&key);
        if was_bold {
            self.row_changed(shown);
        }
    }

    fn row_count(&self, _parent: &QModelIndex) -> i32 {
        row(self.fold.len())
    }

    fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(shown) = usize::try_from(index.row())
            .ok()
            .filter(|shown| *shown < self.fold.len())
        else {
            return QVariant::default();
        };
        let text = |value: &str| QVariant::from(&QString::from(value));
        match self.fold.row(shown) {
            Row::Heading => match role {
                ROW_TYPE_ROLE => text("drafts"),
                DRAFTS_TITLE_ROLE => text(&format!("Drafts {}", self.fold.drafts)),
                DRAFTS_OPEN_ROLE => QVariant::from(&self.fold.open),
                _ => QVariant::default(),
            },
            Row::Pull(at) => {
                let Some(entry) = self.entries.get(at) else {
                    return QVariant::default();
                };
                let view = &entry.view;
                match role {
                    ROW_TYPE_ROLE => text("pull"),
                    PR_ID_ROLE => text(&entry.id()),
                    GLYPH_ROLE => text(glyph_name(view.glyph)),
                    REPO_LINE_ROLE => text(&entry.repo_line()),
                    AGE_ROLE => text(&view.age),
                    BELL_ROLE => QVariant::from(&view.bell),
                    TITLE_ROLE => text(&entry.title_styled()),
                    NUMBER_ROLE => text(&view.number_text),
                    BAR_ROLE => {
                        let names: QStringList = view
                            .bar
                            .iter()
                            .map(|segment| QString::from(segment_name(*segment)))
                            .collect();
                        QVariant::from(&names)
                    }
                    CHANGED_ROLE => QVariant::from(&view.changed),
                    _ => QVariant::default(),
                }
            }
        }
    }

    #[allow(clippy::unused_self)] // Signature fixed by `QAbstractItemModel::roleNames`.
    fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut names = QHash::<QHashPair_i32_QByteArray>::default();
        for (role, name) in [
            (ROW_TYPE_ROLE, "rowType"),
            (PR_ID_ROLE, "prId"),
            (GLYPH_ROLE, "glyph"),
            (REPO_LINE_ROLE, "repoLine"),
            (AGE_ROLE, "age"),
            (BELL_ROLE, "bell"),
            (TITLE_ROLE, "titleStyled"),
            (NUMBER_ROLE, "number"),
            (BAR_ROLE, "bar"),
            (CHANGED_ROLE, "changed"),
            (DRAFTS_TITLE_ROLE, "draftsTitle"),
            (DRAFTS_OPEN_ROLE, "draftsOpen"),
        ] {
            names.insert(role, QByteArray::from(name));
        }
        names
    }
}

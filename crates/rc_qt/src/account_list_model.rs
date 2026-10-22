// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The `AccountList` model: the core's accounts in the order they were added, one row each,
//! updated one row at a time. The hub decides what changes (see [`crate::accounts`]); this only
//! announces it to the views.

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

    /// Where an account is. Logged out also covers an account that has not said anything yet.
    #[qenum(AccountListModel)]
    enum AccountStatus {
        LoggedOut,
        Connecting,
        Resuming,
        LoggedIn,
        /// Logged out because the server refused the token.
        Expired,
    }

    unsafe extern "RustQt" {
        /// Available in QML as the `AccountList` singleton.
        #[qobject]
        #[qml_element = "AccountList"]
        #[qml_singleton]
        #[base = QAbstractListModel]
        // A row is a new account, waiting for its first login.
        #[qproperty(bool, has_new, cxx_name = "hasNew", READ, NOTIFY)]
        type AccountListModel = super::AccountListModelRust;

        /// Shows the account: its rooms, open room and thread replace the ones shown.
        #[qinvokable]
        fn select(self: Pin<&mut AccountListModel>, account_id: &QString);

        /// Adds an account, logged out, and shows it, so that the login page shows for it.
        #[qinvokable]
        fn add(self: Pin<&mut AccountListModel>);

        /// Logs out of the account, if it is logged in, and forgets it. If it was the last one, a
        /// new logged-out account takes its place.
        #[qinvokable]
        fn remove(self: Pin<&mut AccountListModel>, account_id: &QString);

        #[qsignal]
        #[inherit]
        #[cxx_name = "dataChanged"]
        fn data_changed(
            self: Pin<&mut AccountListModel>,
            top_left: &QModelIndex,
            bottom_right: &QModelIndex,
            roles: &QVector_i32,
        );

        #[inherit]
        fn index(
            self: &AccountListModel,
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
            self: Pin<&mut AccountListModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        /// # Safety
        ///
        /// Only after `begin_insert_rows`.
        #[inherit]
        #[cxx_name = "endInsertRows"]
        unsafe fn end_insert_rows(self: Pin<&mut AccountListModel>);

        /// # Safety
        ///
        /// Call `end_remove_rows` after changing the data.
        #[inherit]
        #[cxx_name = "beginRemoveRows"]
        unsafe fn begin_remove_rows(
            self: Pin<&mut AccountListModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        /// # Safety
        ///
        /// Only after `begin_remove_rows`.
        #[inherit]
        #[cxx_name = "endRemoveRows"]
        unsafe fn end_remove_rows(self: Pin<&mut AccountListModel>);
    }

    extern "RustQt" {
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &AccountListModel, parent: &QModelIndex) -> i32;

        #[cxx_override]
        fn data(self: &AccountListModel, index: &QModelIndex, role: i32) -> QVariant;

        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &AccountListModel) -> QHash_i32_QByteArray;
    }

    impl cxx_qt::Threading for AccountListModel {}
    impl cxx_qt::Initialize for AccountListModel {}
}

use core::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{
    QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant, QVector,
};

use crate::accounts::{AccountChange, AccountView, Status};
use crate::hub;

use qobject::{AccountListModel, AccountStatus};

// The custom roles, from `Qt::UserRole` on. Plain constants, as cxx-qt's `qenum` can't set values
// and the default ones would start at `Qt::DisplayRole`.
const ACCOUNT_ID_ROLE: i32 = 256;
const SERVER_ROLE: i32 = 257;
const HOST_ROLE: i32 = 258;
const USERNAME_ROLE: i32 = 259;
const STATUS_ROLE: i32 = 260;
const CONNECTED_ROLE: i32 = 261;
const UNREAD_ROOMS_ROLE: i32 = 262;
const MENTIONS_ROLE: i32 = 263;
const CURRENT_ROLE: i32 = 264;
const MONOGRAM_ROLE: i32 = 265;
const BUSY_ROLE: i32 = 266;
const UNREAD_ROLE: i32 = 267;
const FRESH_ROLE: i32 = 268;
const TIP_TITLE_ROLE: i32 = 269;
const TIP_LINE_ROLE: i32 = 270;
const LABEL_ROLE: i32 = 271;

/// Backing data of [`qobject::AccountListModel`]: the rows, mirrored from the hub's changes.
#[derive(Default)]
pub struct AccountListModelRust {
    rows: Vec<AccountView>,
    has_new: bool,
}

/// Whether one of the rows is a new account: the rail has a dashed plus for it, and no use for
/// another one.
fn has_new(rows: &[AccountView]) -> bool {
    rows.iter().any(|row| row.fresh)
}

/// Counts are `u32`, Qt's are `int`: saturate.
fn count(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

/// `index` as the `int` Qt wants. Accounts never come near `i32::MAX`.
fn row(index: usize) -> i32 {
    i32::try_from(index).unwrap_or(i32::MAX)
}

const fn status(status: Status) -> AccountStatus {
    match status {
        Status::LoggedOut => AccountStatus::LoggedOut,
        Status::Connecting => AccountStatus::Connecting,
        Status::Resuming => AccountStatus::Resuming,
        Status::LoggedIn => AccountStatus::LoggedIn,
        Status::Expired => AccountStatus::Expired,
    }
}

impl cxx_qt::Initialize for AccountListModel {
    fn initialize(mut self: Pin<&mut Self>) {
        // Nothing is looking at the model yet, so there is nothing to announce.
        let rows = hub::register_accounts(self.qt_thread());
        self.as_mut().rust_mut().has_new = has_new(&rows);
        self.as_mut().rust_mut().rows = rows;
    }
}

impl AccountListModel {
    /// Applies the change, announcing it to the views.
    pub fn apply(mut self: Pin<&mut Self>, change: &AccountChange) {
        self.as_mut().apply_rows(change);
        let new = has_new(&self.rows);
        if new != self.has_new {
            self.as_mut().rust_mut().has_new = new;
            self.as_mut().has_new_changed();
        }
    }

    fn apply_rows(mut self: Pin<&mut Self>, change: &AccountChange) {
        let no_parent = QModelIndex::default();
        match *change {
            AccountChange::Inserted { index, .. } => {
                // SAFETY: balanced by `end_insert_rows` below.
                unsafe {
                    self.as_mut()
                        .begin_insert_rows(&no_parent, row(index), row(index));
                }
                change.apply(&mut self.as_mut().rust_mut().rows);
                // SAFETY: after `begin_insert_rows`.
                unsafe { self.as_mut().end_insert_rows() };
            }
            AccountChange::Updated { index, .. } => {
                change.apply(&mut self.as_mut().rust_mut().rows);
                let at = self.index(row(index), 0, &no_parent);
                // No roles listed means all of them.
                self.as_mut()
                    .data_changed(&at, &at, &QVector::<i32>::default());
            }
            AccountChange::Removed { index } => {
                // SAFETY: balanced by `end_remove_rows` below.
                unsafe {
                    self.as_mut()
                        .begin_remove_rows(&no_parent, row(index), row(index));
                }
                change.apply(&mut self.as_mut().rust_mut().rows);
                // SAFETY: after `begin_remove_rows`.
                unsafe { self.as_mut().end_remove_rows() };
            }
        }
    }

    #[allow(clippy::unused_self)] // The invokable needs an object; the core is process-wide.
    fn select(self: Pin<&mut Self>, account_id: &QString) {
        hub::select_account(&account_id.to_string());
    }

    #[allow(clippy::unused_self)]
    fn add(self: Pin<&mut Self>) {
        hub::add_account();
    }

    #[allow(clippy::unused_self)]
    fn remove(self: Pin<&mut Self>, account_id: &QString) {
        hub::remove_account(&account_id.to_string());
    }

    fn row_count(&self, _parent: &QModelIndex) -> i32 {
        row(self.rows.len())
    }

    fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(account) = usize::try_from(index.row())
            .ok()
            .and_then(|at| self.rows.get(at))
        else {
            return QVariant::default();
        };
        let text = |value: &str| QVariant::from(&QString::from(value));
        match role {
            ACCOUNT_ID_ROLE => text(&account.id),
            SERVER_ROLE => text(&account.server),
            HOST_ROLE => text(&account.host),
            USERNAME_ROLE => text(&account.username),
            STATUS_ROLE => QVariant::from(&status(account.status).repr),
            CONNECTED_ROLE => QVariant::from(&account.connected),
            UNREAD_ROOMS_ROLE => QVariant::from(&count(account.unread_rooms)),
            MENTIONS_ROLE => QVariant::from(&count(account.mentions)),
            CURRENT_ROLE => QVariant::from(&account.current),
            MONOGRAM_ROLE => text(&account.monogram),
            BUSY_ROLE => QVariant::from(&account.busy),
            UNREAD_ROLE => QVariant::from(&account.unread),
            FRESH_ROLE => QVariant::from(&account.fresh),
            TIP_TITLE_ROLE => text(&account.tip_title),
            TIP_LINE_ROLE => text(&account.tip_line),
            LABEL_ROLE => text(&account.label),
            _ => QVariant::default(),
        }
    }

    #[allow(clippy::unused_self)] // Signature fixed by `QAbstractItemModel::roleNames`.
    fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut names = QHash::<QHashPair_i32_QByteArray>::default();
        for (role, name) in [
            (ACCOUNT_ID_ROLE, "accountId"),
            (SERVER_ROLE, "server"),
            (HOST_ROLE, "host"),
            (USERNAME_ROLE, "username"),
            (STATUS_ROLE, "status"),
            (CONNECTED_ROLE, "connected"),
            (UNREAD_ROOMS_ROLE, "unreadRooms"),
            (MENTIONS_ROLE, "mentions"),
            (CURRENT_ROLE, "current"),
            (MONOGRAM_ROLE, "monogram"),
            (BUSY_ROLE, "busy"),
            (UNREAD_ROLE, "unread"),
            (FRESH_ROLE, "fresh"),
            (TIP_TITLE_ROLE, "tipTitle"),
            (TIP_LINE_ROLE, "tipLine"),
            (LABEL_ROLE, "label"),
        ] {
            names.insert(role, QByteArray::from(name));
        }
        names
    }
}

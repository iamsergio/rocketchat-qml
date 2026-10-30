// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The `MessageSearch` model: the pane that searches the open room's messages (MSG-008,
//! `docs/design/message-search.md`). The field's text goes to the core, which searches the server;
//! `rc_view::message_search_view` decides what the pane shows, and this lists the rows and
//! publishes the rest as properties. The pane being open is the core having a search: the model
//! has no flag of its own for it.

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
        /// Available in QML as the `MessageSearch` singleton.
        #[qobject]
        #[qml_element = "MessageSearch"]
        #[qml_singleton]
        #[base = QAbstractListModel]
        // `empty`, `searching`, `none`, `failed` or `results`: what the list area is about.
        #[qproperty(QString, status, READ, NOTIFY)]
        // The name of the room searched, for the header.
        #[qproperty(QString, room_name, cxx_name = "roomName", READ, NOTIFY)]
        // The title and the hint of the note of an empty field or of no result.
        #[qproperty(QString, note_title, cxx_name = "noteTitle", READ, NOTIFY)]
        #[qproperty(QString, note_hint, cxx_name = "noteHint", READ, NOTIFY)]
        // Why the first page failed.
        #[qproperty(QString, error, READ, NOTIFY)]
        // The hint under the field; a search runs while rows are on show if `refreshing`.
        #[qproperty(QString, field_hint, cxx_name = "fieldHint", READ, NOTIFY)]
        #[qproperty(bool, refreshing, READ, NOTIFY)]
        // `nothing`, `available`, `loading` or `failed`: what is under the last row.
        #[qproperty(QString, footer, READ, NOTIFY)]
        #[qproperty(QString, footer_error, cxx_name = "footerError", READ, NOTIFY)]
        // The row Enter would show, or -1 for none: always a message.
        #[qproperty(i32, cursor, READ, NOTIFY)]
        type MessageSearchModel = super::MessageSearchModelRust;

        /// The field's text changed: searches the open room for it. Ignored while the pane is
        /// closed.
        #[qinvokable]
        #[cxx_name = "setQuery"]
        fn set_query(self: Pin<&mut MessageSearchModel>, text: &QString);

        /// Searches the text of the field again, after a failure.
        #[qinvokable]
        fn retry(self: Pin<&mut MessageSearchModel>);

        /// The list reached its end: asks for the next page if there is one and rows are shown.
        /// QML calls it whenever the end is in view.
        #[qinvokable]
        #[cxx_name = "reachedEnd"]
        fn reached_end(self: Pin<&mut MessageSearchModel>);

        /// Asks for the page again, after a failure of it.
        #[qinvokable]
        #[cxx_name = "retryMore"]
        fn retry_more(self: Pin<&mut MessageSearchModel>);

        /// Moves the cursor to the previous message, wrapping.
        #[qinvokable]
        #[cxx_name = "cursorUp"]
        fn cursor_up(self: Pin<&mut MessageSearchModel>);

        /// Moves the cursor to the next message, wrapping.
        #[qinvokable]
        #[cxx_name = "cursorDown"]
        fn cursor_down(self: Pin<&mut MessageSearchModel>);

        /// Puts the cursor on `row` if it is a message (the pointer is over it).
        #[qinvokable]
        #[cxx_name = "setCursor"]
        fn set_cursor_to(self: Pin<&mut MessageSearchModel>, row: i32);

        /// Enter, or a click, on `row` (-1 for the cursor's). Only moves the cursor for now: showing
        /// the message in the room is the next step of MSG-008.
        #[qinvokable]
        fn pick(self: Pin<&mut MessageSearchModel>, row: i32);

        /// The room changed under the pane: it searches nothing in the new one, and the field is
        /// to be emptied.
        #[qsignal]
        #[cxx_name = "fieldReset"]
        fn field_reset(self: Pin<&mut MessageSearchModel>);

        #[qsignal]
        #[inherit]
        #[cxx_name = "dataChanged"]
        fn data_changed(
            self: Pin<&mut MessageSearchModel>,
            top_left: &QModelIndex,
            bottom_right: &QModelIndex,
            roles: &QVector_i32,
        );

        #[inherit]
        fn index(
            self: &MessageSearchModel,
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
            self: Pin<&mut MessageSearchModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        /// # Safety
        ///
        /// Only after `begin_insert_rows`.
        #[inherit]
        #[cxx_name = "endInsertRows"]
        unsafe fn end_insert_rows(self: Pin<&mut MessageSearchModel>);

        /// # Safety
        ///
        /// Call `end_reset_model` after changing the data.
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut MessageSearchModel>);
        /// # Safety
        ///
        /// Only after `begin_reset_model`.
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut MessageSearchModel>);
    }

    extern "RustQt" {
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &MessageSearchModel, parent: &QModelIndex) -> i32;

        #[cxx_override]
        fn data(self: &MessageSearchModel, index: &QModelIndex, role: i32) -> QVariant;

        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &MessageSearchModel) -> QHash_i32_QByteArray;
    }

    impl cxx_qt::Threading for MessageSearchModel {}
    impl cxx_qt::Initialize for MessageSearchModel {}
}

use core::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{
    QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant, QVector,
};
use jiff::Timestamp;
use jiff::tz::TimeZone;
use rc_core::domain::AvatarOwner;
use rc_view::avatar_view::{AvatarUrls, color, initials, owners, rows_using};
use rc_view::message_search_view::{Footer, Row, View, first_cursor, message_at, step_cursor};

use crate::hub;
use crate::room_roles::row;

use qobject::MessageSearchModel;

const ROW_TYPE_ROLE: i32 = 256;
const LABEL_ROLE: i32 = 257;
const MESSAGE_ID_ROLE: i32 = 258;
const AUTHOR_ROLE: i32 = 259;
const TIME_ROLE: i32 = 260;
const TEXT_ROLE: i32 = 261;
const IN_THREAD_ROLE: i32 = 262;
/// The only role that changes when an avatar file arrives.
const AVATAR_ROLE: i32 = 263;
const AVATAR_INITIALS_ROLE: i32 = 264;
const AVATAR_COLOR_ROLE: i32 = 265;
const PICKED_ROLE: i32 = 266;

/// Backing data of [`qobject::MessageSearchModel`].
pub struct MessageSearchModelRust {
    status: QString,
    room_name: QString,
    note_title: QString,
    note_hint: QString,
    error: QString,
    field_hint: QString,
    refreshing: bool,
    footer: QString,
    footer_error: QString,
    cursor: i32,
    avatars: AvatarUrls,
    rows: Vec<Row>,
    /// The text of the field, as the pane last said.
    text: String,
    /// Where times and days are read: the user's.
    zone: TimeZone,
}

impl Default for MessageSearchModelRust {
    fn default() -> Self {
        Self {
            status: QString::from("empty"),
            room_name: QString::default(),
            note_title: QString::default(),
            note_hint: QString::default(),
            error: QString::default(),
            field_hint: QString::default(),
            refreshing: false,
            footer: QString::from("nothing"),
            footer_error: QString::default(),
            cursor: -1,
            avatars: AvatarUrls::default(),
            rows: Vec::new(),
            text: String::new(),
            zone: TimeZone::system(),
        }
    }
}

impl cxx_qt::Initialize for MessageSearchModel {
    fn initialize(self: Pin<&mut Self>) {
        hub::register_message_search(self.qt_thread());
        self.refresh();
    }
}

/// Sets the property `field` of `this` to `value` and emits `signal` if that changed it.
macro_rules! sync {
    ($this:ident, $field:ident, $value:expr, $signal:ident) => {{
        let value = $value;
        if $this.$field != value {
            $this.as_mut().rust_mut().$field = value;
            $this.as_mut().$signal();
        }
    }};
}

impl MessageSearchModel {
    /// The core said something of the search, the room changed or the account switched: shows the
    /// pane again. A search that is of another room than the open one moves to it, empty.
    pub fn refresh(mut self: Pin<&mut Self>) {
        let now = Timestamp::now();
        let zone = self.zone.clone();
        let (view, avatars, stale) = hub::message_search_shown(&zone, now);
        self.as_mut().rust_mut().avatars = avatars;
        self.as_mut().show(view);
        if let Some(room) = stale {
            self.as_mut().rust_mut().text.clear();
            hub::search_messages(&room, "");
            self.field_reset();
        }
    }

    /// Announces new content in the role of the avatar for the rows that show the new files.
    pub fn set_avatars(mut self: Pin<&mut Self>, changes: &[(AvatarOwner, String)]) {
        self.as_mut().rust_mut().avatars.set(changes);
        let runs = rows_using(self.rows.len(), &owners(changes), |at| {
            match self.rows.get(at) {
                Some(Row::Message(found)) => found.avatar.clone(),
                _ => None,
            }
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

    /// Takes `view` in: the rows, announced as an insert when it only adds to the ones held (the
    /// next page, which keeps the list where it is) and else as a reset, and the properties.
    fn show(mut self: Pin<&mut Self>, view: View) {
        self.as_mut().set_rows(view.rows);
        sync!(
            self,
            status,
            QString::from(view.status.name()),
            status_changed
        );
        sync!(
            self,
            room_name,
            QString::from(&view.room_name),
            room_name_changed
        );
        sync!(
            self,
            note_title,
            QString::from(&view.note_title),
            note_title_changed
        );
        sync!(
            self,
            note_hint,
            QString::from(&view.note_hint),
            note_hint_changed
        );
        sync!(self, error, QString::from(&view.error), error_changed);
        sync!(
            self,
            field_hint,
            QString::from(&view.field_hint),
            field_hint_changed
        );
        sync!(self, refreshing, view.refreshing, refreshing_changed);
        sync!(
            self,
            footer,
            QString::from(view.footer.name()),
            footer_changed
        );
        let footer_error = match &view.footer {
            Footer::Failed(text) => text.as_str(),
            _ => "",
        };
        sync!(
            self,
            footer_error,
            QString::from(footer_error),
            footer_error_changed
        );
    }

    fn set_rows(mut self: Pin<&mut Self>, rows: Vec<Row>) {
        let held = self.rows.len();
        if rows == self.rows {
            return;
        }
        if rows.len() > held && rows[..held] == self.rows[..] {
            let last = rows.len() - 1;
            // SAFETY: balanced by `end_insert_rows` below.
            unsafe {
                self.as_mut()
                    .begin_insert_rows(&QModelIndex::default(), row(held), row(last));
            }
            self.as_mut().rust_mut().rows = rows;
            // SAFETY: after `begin_insert_rows`.
            unsafe { self.as_mut().end_insert_rows() };
            if held > 0 {
                return;
            }
        } else {
            // SAFETY: balanced by `end_reset_model` below.
            unsafe { self.as_mut().begin_reset_model() };
            self.as_mut().rust_mut().rows = rows;
            // SAFETY: after `begin_reset_model`.
            unsafe { self.as_mut().end_reset_model() };
        }
        // New results: the cursor is on the first message, where Enter will go.
        let first = first_cursor(&self.rows).map_or(-1, row);
        sync!(self, cursor, first, cursor_changed);
    }

    fn set_query(mut self: Pin<&mut Self>, text: &QString) {
        self.as_mut().rust_mut().text = text.to_string();
        self.search_again();
    }

    fn retry(self: Pin<&mut Self>) {
        self.search_again();
    }

    /// Searches the open room for the text of the field, if the pane is open.
    #[allow(clippy::unused_self)]
    fn search_again(self: Pin<&mut Self>) {
        if let Some(room) = hub::message_search_room() {
            hub::search_messages(&room, &self.text);
        }
    }

    fn reached_end(self: Pin<&mut Self>) {
        let available = self.footer == QString::from(Footer::Available.name());
        if available && !self.rows.is_empty() {
            hub::more_message_search();
        }
    }

    #[allow(clippy::unused_self)]
    fn retry_more(self: Pin<&mut Self>) {
        hub::more_message_search();
    }

    fn cursor_up(self: Pin<&mut Self>) {
        self.move_cursor(false);
    }

    fn cursor_down(self: Pin<&mut Self>) {
        self.move_cursor(true);
    }

    fn move_cursor(mut self: Pin<&mut Self>, down: bool) {
        let from = usize::try_from(self.cursor).ok();
        if let Some(to) = step_cursor(&self.rows, from, down) {
            sync!(self, cursor, row(to), cursor_changed);
        }
    }

    fn set_cursor_to(mut self: Pin<&mut Self>, at: i32) {
        let on_message = usize::try_from(at).is_ok_and(|at| message_at(&self.rows, at).is_some());
        if on_message {
            sync!(self, cursor, at, cursor_changed);
        }
    }

    fn pick(self: Pin<&mut Self>, at: i32) {
        let at = if at < 0 { self.cursor } else { at };
        // Step 3 of MSG-008 shows the message in the room from here.
        self.set_cursor_to(at);
    }

    fn row_count(&self, _parent: &QModelIndex) -> i32 {
        row(self.rows.len())
    }

    fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(found) = usize::try_from(index.row())
            .ok()
            .and_then(|at| self.rows.get(at))
        else {
            return QVariant::default();
        };
        let text = |text: &str| QVariant::from(&QString::from(text));
        match (found, role) {
            (Row::Day(_), ROW_TYPE_ROLE) => text("day"),
            (Row::Message(_), ROW_TYPE_ROLE) => text("message"),
            (Row::Day(label), LABEL_ROLE) => text(label),
            (Row::Message(found), MESSAGE_ID_ROLE) => text(found.id.as_str()),
            (Row::Message(found), AUTHOR_ROLE) => text(&found.author),
            (Row::Message(found), TIME_ROLE) => text(&found.time),
            (Row::Message(found), TEXT_ROLE) => text(&found.text),
            (Row::Message(found), IN_THREAD_ROLE) => QVariant::from(&found.in_thread),
            (Row::Message(found), AVATAR_ROLE) => text(
                found
                    .avatar
                    .as_ref()
                    .map_or("", |owner| self.avatars.url(owner)),
            ),
            (Row::Message(found), AVATAR_INITIALS_ROLE) => text(&initials(&found.author)),
            (Row::Message(found), AVATAR_COLOR_ROLE) => text(&color(&found.username)),
            // The delegate reads every role of every row, so a row answers the ones it has no
            // use for with nothing.
            (_, IN_THREAD_ROLE | PICKED_ROLE) => QVariant::from(&false),
            (_, LABEL_ROLE..=AVATAR_COLOR_ROLE) => text(""),
            _ => QVariant::default(),
        }
    }

    #[allow(clippy::unused_self)] // Signature fixed by `QAbstractItemModel::roleNames`.
    fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut names = QHash::<QHashPair_i32_QByteArray>::default();
        for (role, name) in [
            (ROW_TYPE_ROLE, "rowType"),
            (LABEL_ROLE, "label"),
            (MESSAGE_ID_ROLE, "messageId"),
            (AUTHOR_ROLE, "author"),
            (TIME_ROLE, "time"),
            (TEXT_ROLE, "body"),
            (IN_THREAD_ROLE, "inThread"),
            (AVATAR_ROLE, "avatar"),
            (AVATAR_INITIALS_ROLE, "avatarInitials"),
            (AVATAR_COLOR_ROLE, "avatarColor"),
            // The row picked to be shown in the room; step 3 of MSG-008 sets it.
            (PICKED_ROLE, "picked"),
        ] {
            names.insert(role, QByteArray::from(name));
        }
        names
    }
}

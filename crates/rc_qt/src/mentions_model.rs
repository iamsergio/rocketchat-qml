// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The `Mentions` model: the popup above a composer that completes an `@word` with a member of the
//! room (ACT-013, `docs/design/mentions.md`). The composers tell it their text and cursor; it
//! decides with `rc_view::mention` whether the popup is open, searches the room's members through
//! the core, and lists the rows. One popup serves both composers (the room's and the thread's):
//! only one has the focus at a time, and the one that drives it is its `owner`.

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
        /// Available in QML as the `Mentions` singleton.
        #[qobject]
        #[qml_element = "Mentions"]
        #[qml_singleton]
        #[base = QAbstractListModel]
        // Whether the popup is open.
        #[qproperty(bool, open, READ, NOTIFY)]
        // `room` or `thread`: the composer that drives the popup. Only that one shows it.
        #[qproperty(QString, owner, READ, NOTIFY)]
        // The row Enter and Tab insert, or -1 for none: always a person or `@all`/`@here`.
        #[qproperty(i32, cursor, READ, NOTIFY)]
        // `idle`, `searching` (nothing to show yet) or `failed` (the rows are what remains).
        #[qproperty(QString, status, READ, NOTIFY)]
        #[qproperty(QString, failed_text, cxx_name = "failedText", READ, NOTIFY)]
        // The room is a direct message: the label says so.
        #[qproperty(bool, direct, READ, NOTIFY)]
        // What is typed after the `@`, for the line that says nobody matches.
        #[qproperty(QString, query, READ, NOTIFY)]
        type MentionsModel = super::MentionsModelRust;

        /// The composer `owner` (`room` or `thread`) says what its field holds: the text and
        /// where the cursor is (UTF-16 units). Opens, updates or closes the popup.
        #[qinvokable]
        fn update(self: Pin<&mut MentionsModel>, owner: &QString, text: &QString, cursor: i32);

        /// Closes the popup if `owner` drives it: the composer lost the focus, the room or the
        /// thread changed, or a message was sent.
        #[qinvokable]
        fn close(self: Pin<&mut MentionsModel>, owner: &QString);

        /// Esc: closes the popup and keeps it closed for this word until its text changes.
        /// Returns whether it was open, i.e. whether the key is taken.
        #[qinvokable]
        fn escape(self: Pin<&mut MentionsModel>) -> bool;

        /// Moves the cursor to the previous choosable row, wrapping.
        #[qinvokable]
        #[cxx_name = "cursorUp"]
        fn cursor_up(self: Pin<&mut MentionsModel>);

        /// Moves the cursor to the next choosable row, wrapping.
        #[qinvokable]
        #[cxx_name = "cursorDown"]
        fn cursor_down(self: Pin<&mut MentionsModel>);

        /// Puts the cursor on `row` if it can be chosen (the pointer is over it).
        #[qinvokable]
        #[cxx_name = "setCursor"]
        fn set_cursor_to(self: Pin<&mut MentionsModel>, row: i32);

        /// Inserts the row's `@username ` in the field: tells it with `inserted`. Returns whether
        /// the row could be chosen.
        #[qinvokable]
        fn accept(self: Pin<&mut MentionsModel>, row: i32) -> bool;

        /// `accept` for the row the cursor is on.
        #[qinvokable]
        #[cxx_name = "acceptCursor"]
        fn accept_cursor(self: Pin<&mut MentionsModel>) -> bool;

        /// The field is to replace its characters `start` to `end` with `replacement` and put its
        /// cursor at `cursor` (UTF-16 units). A replacement of the range, not of the whole text,
        /// so that the field's undo stack goes on.
        #[qsignal]
        fn inserted(
            self: Pin<&mut MentionsModel>,
            start: i32,
            end: i32,
            replacement: &QString,
            cursor: i32,
        );

        #[qsignal]
        #[inherit]
        #[cxx_name = "dataChanged"]
        fn data_changed(
            self: Pin<&mut MentionsModel>,
            top_left: &QModelIndex,
            bottom_right: &QModelIndex,
            roles: &QVector_i32,
        );

        #[inherit]
        fn index(self: &MentionsModel, row: i32, column: i32, parent: &QModelIndex) -> QModelIndex;
    }

    // The begin/end pair must be kept balanced, so they are unsafe to call.
    extern "RustQt" {
        /// # Safety
        ///
        /// Call `end_reset_model` after changing the data.
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut MentionsModel>);
        /// # Safety
        ///
        /// Only after `begin_reset_model`.
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut MentionsModel>);
    }

    extern "RustQt" {
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &MentionsModel, parent: &QModelIndex) -> i32;

        #[cxx_override]
        fn data(self: &MentionsModel, index: &QModelIndex, role: i32) -> QVariant;

        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &MentionsModel) -> QHash_i32_QByteArray;
    }

    impl cxx_qt::Threading for MentionsModel {}
    impl cxx_qt::Initialize for MentionsModel {}
}

use core::pin::Pin;
use std::ops::Range;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{
    QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant, QVector,
};
use rc_core::domain::{AvatarOwner, RoomId};
use rc_view::avatar_view::{AvatarUrls, color, initials, owners, rows_using};
use rc_view::mention::{self, FAILED_TEXT, MentionWord, Row, Status};

use crate::hub;
use crate::room_roles::row;

use qobject::MentionsModel;

const ROW_TYPE_ROLE: i32 = 256;
const USERNAME_ROLE: i32 = 257;
const NAME_ROLE: i32 = 258;
const NAME_STYLED_ROLE: i32 = 259;
const USERNAME_STYLED_ROLE: i32 = 260;
const AVATAR_ROLE: i32 = 261;
const AVATAR_INITIALS_ROLE: i32 = 262;
const AVATAR_COLOR_ROLE: i32 = 263;
const COUNT_ROLE: i32 = 264;
const DESCRIPTION_ROLE: i32 = 265;

/// Backing data of [`qobject::MentionsModel`].
pub struct MentionsModelRust {
    avatars: AvatarUrls,
    rows: Vec<Row>,
    open: bool,
    owner: QString,
    cursor: i32,
    status: QString,
    failed_text: QString,
    direct: bool,
    query: QString,
    /// The text of the field and the cursor in it, as the owner last said.
    text: String,
    cursor_at: usize,
    /// The user moved the cursor (with the keys or the pointer) since the query last changed: only
    /// then does it stay on its row when the rows change, else it goes to the first.
    moved: bool,
    /// The word the popup completes.
    word: Option<MentionWord>,
    /// The room and query the core was last asked about, to ask only when they change.
    searched: Option<(RoomId, String)>,
    /// The word Esc closed the popup for (where it starts and the whole text), which stays closed
    /// until the text changes.
    dismissed: Option<(usize, String)>,
}

impl Default for MentionsModelRust {
    fn default() -> Self {
        Self {
            avatars: AvatarUrls::default(),
            rows: Vec::new(),
            open: false,
            owner: QString::default(),
            cursor: -1,
            status: QString::from("idle"),
            failed_text: QString::default(),
            direct: false,
            query: QString::default(),
            text: String::new(),
            cursor_at: 0,
            moved: false,
            word: None,
            searched: None,
            dismissed: None,
        }
    }
}

impl cxx_qt::Initialize for MentionsModel {
    fn initialize(self: Pin<&mut Self>) {
        hub::register_mentions(self.qt_thread());
    }
}

/// `text` for a styled label: escaped, with `<b>` around `range` (a byte range of `text`).
fn styled(text: &str, range: Option<&Range<usize>>) -> String {
    let escape = |part: &str| {
        part.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    };
    match range.filter(|r| text.get(r.start..r.end).is_some()) {
        Some(range) => format!(
            "{}<b>{}</b>{}",
            escape(&text[..range.start]),
            escape(&text[range.clone()]),
            escape(&text[range.end..])
        ),
        None => escape(text),
    }
}

fn avatar_owner(found: &Row) -> Option<AvatarOwner> {
    match found {
        Row::Person(person) => Some(person.avatar.clone()),
        _ => None,
    }
}

impl MentionsModel {
    /// The core said something of the member search, or the account switched: shows the rows
    /// again.
    pub fn refresh(mut self: Pin<&mut Self>) {
        if self.open {
            self.as_mut().rebuild();
        }
    }

    /// Announces new content in the role of the avatar for the rows that show the new files.
    pub fn set_avatars(mut self: Pin<&mut Self>, changes: &[(AvatarOwner, String)]) {
        self.as_mut().rust_mut().avatars.set(changes);
        let runs = rows_using(self.rows.len(), &owners(changes), |at| {
            self.rows.get(at).and_then(avatar_owner)
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

    fn update(mut self: Pin<&mut Self>, owner: &QString, text: &QString, cursor: i32) {
        let text = text.to_string();
        let at = usize::try_from(cursor).unwrap_or(0);
        let word = mention::word_at_cursor(&text, at);
        // Esc closed the popup for this word; any change of the text opens it again.
        let still_dismissed = match (&word, &self.dismissed) {
            (Some(word), Some((start, dismissed))) => word.start == *start && *dismissed == text,
            _ => false,
        };
        if !still_dismissed {
            self.as_mut().rust_mut().dismissed = None;
        }
        let word = word.filter(|_| !still_dismissed);
        let for_thread = owner.to_string() == "thread";
        let target = word.as_ref().and_then(|_| hub::mention_target(for_thread));
        let Some((word, (room, direct))) = word.zip(target) else {
            self.close_popup();
            return;
        };
        {
            let mut this = self.as_mut().rust_mut();
            this.text = text;
            this.cursor_at = at;
        }
        self.as_mut().set_owner_value(owner);
        self.as_mut().set_direct_value(direct);
        self.as_mut().set_query_value(&word.query);
        let asked = (room.clone(), word.query.clone());
        let query_changed = self.word.as_ref().map(|old| &old.query) != Some(&word.query);
        if query_changed {
            self.as_mut().rust_mut().moved = false;
        }
        self.as_mut().rust_mut().word = Some(word);
        if self.searched.as_ref() != Some(&asked) {
            hub::search_members(&room, &asked.1);
            self.as_mut().rust_mut().searched = Some(asked);
        }
        self.as_mut().rebuild();
        self.as_mut().set_open_value(true);
    }

    fn close(mut self: Pin<&mut Self>, owner: &QString) {
        if self.open && self.owner == *owner {
            self.as_mut().rust_mut().dismissed = None;
            self.close_popup();
        }
    }

    fn escape(mut self: Pin<&mut Self>) -> bool {
        if !self.open {
            return false;
        }
        let dismissed = (self.word.as_ref()).map(|word| (word.start, self.text.clone()));
        self.as_mut().rust_mut().dismissed = dismissed;
        self.close_popup();
        true
    }

    /// Closes the popup and ends the search that fed it.
    fn close_popup(mut self: Pin<&mut Self>) {
        if self.as_mut().rust_mut().searched.take().is_some() {
            hub::cancel_member_search();
        }
        self.as_mut().rust_mut().word = None;
        if !self.rows.is_empty() {
            // SAFETY: balanced by `end_reset_model` below.
            unsafe { self.as_mut().begin_reset_model() };
            self.as_mut().rust_mut().rows.clear();
            // SAFETY: after `begin_reset_model`.
            unsafe { self.as_mut().end_reset_model() };
        }
        self.as_mut().set_cursor_value(-1);
        self.set_open_value(false);
    }

    /// Lists the popup for the query now. The cursor stays on its person if they are still
    /// listed, else it goes to the first row that can be chosen.
    fn rebuild(mut self: Pin<&mut Self>) {
        let query = self.word.as_ref().map(|word| word.query.clone());
        let Some(query) = query else {
            return;
        };
        let kept = usize::try_from(self.cursor)
            .ok()
            .filter(|_| self.moved)
            .and_then(|at| self.rows.get(at))
            .and_then(|found| found.username().map(str::to_owned));
        let (popup, avatars) = hub::mention_popup(self.direct, &query);
        let at = kept
            .and_then(|name| popup.rows.iter().position(|r| r.username() == Some(&name)))
            .or_else(|| popup.rows.iter().position(|r| r.username().is_some()));
        // SAFETY: balanced by `end_reset_model` below.
        unsafe { self.as_mut().begin_reset_model() };
        self.as_mut().rust_mut().rows = popup.rows;
        self.as_mut().rust_mut().avatars = avatars;
        // SAFETY: after `begin_reset_model`.
        unsafe { self.as_mut().end_reset_model() };
        self.as_mut().set_cursor_value(at.map_or(-1, row));
        let status = match popup.status {
            Status::Idle => "idle",
            Status::Searching => "searching",
            Status::Failed => "failed",
        };
        self.as_mut().set_status_value(status);
        let failed = if popup.status == Status::Failed {
            FAILED_TEXT
        } else {
            ""
        };
        self.set_failed_text_value(failed);
    }

    fn set_open_value(mut self: Pin<&mut Self>, open: bool) {
        if self.open != open {
            self.as_mut().rust_mut().open = open;
            self.open_changed();
        }
    }

    fn set_owner_value(mut self: Pin<&mut Self>, owner: &QString) {
        if self.owner != *owner {
            self.as_mut().rust_mut().owner = owner.clone();
            self.owner_changed();
        }
    }

    fn set_direct_value(mut self: Pin<&mut Self>, direct: bool) {
        if self.direct != direct {
            self.as_mut().rust_mut().direct = direct;
            self.direct_changed();
        }
    }

    fn set_query_value(mut self: Pin<&mut Self>, query: &str) {
        let query = QString::from(query);
        if self.query != query {
            self.as_mut().rust_mut().query = query;
            self.query_changed();
        }
    }

    fn set_status_value(mut self: Pin<&mut Self>, status: &str) {
        let status = QString::from(status);
        if self.status != status {
            self.as_mut().rust_mut().status = status;
            self.status_changed();
        }
    }

    fn set_failed_text_value(mut self: Pin<&mut Self>, text: &str) {
        let text = QString::from(text);
        if self.failed_text != text {
            self.as_mut().rust_mut().failed_text = text;
            self.failed_text_changed();
        }
    }

    fn set_cursor_value(mut self: Pin<&mut Self>, cursor: i32) {
        if self.cursor != cursor {
            self.as_mut().rust_mut().cursor = cursor;
            self.cursor_changed();
        }
    }

    fn cursor_up(self: Pin<&mut Self>) {
        self.move_cursor(true);
    }

    fn cursor_down(self: Pin<&mut Self>) {
        self.move_cursor(false);
    }

    fn move_cursor(mut self: Pin<&mut Self>, up: bool) {
        let count = self.rows.len();
        let Ok(from) = usize::try_from(self.cursor) else {
            return;
        };
        let to = (1..count)
            .map(|step| {
                if up {
                    (from + count - step) % count
                } else {
                    (from + step) % count
                }
            })
            .find(|at| self.rows[*at].username().is_some());
        if let Some(to) = to {
            self.as_mut().rust_mut().moved = true;
            self.set_cursor_value(row(to));
        }
    }

    fn set_cursor_to(mut self: Pin<&mut Self>, at: i32) {
        let choosable = usize::try_from(at)
            .ok()
            .and_then(|at| self.rows.get(at))
            .is_some_and(|found| found.username().is_some());
        if choosable {
            self.as_mut().rust_mut().moved = true;
            self.set_cursor_value(at);
        }
    }

    fn accept(mut self: Pin<&mut Self>, at: i32) -> bool {
        let Some(word) = self.word.clone() else {
            return false;
        };
        let Some(username) = usize::try_from(at)
            .ok()
            .and_then(|at| self.rows.get(at))
            .and_then(|found| found.username().map(str::to_owned))
        else {
            return false;
        };
        let (new_text, new_cursor) =
            mention::insert(&self.text, self.cursor_at, word.start, &username);
        // What the old and the new text have in common before the word and after the inserted
        // text stays; the field replaces what lies between.
        let old: Vec<u16> = self.text.encode_utf16().collect();
        let new: Vec<u16> = new_text.encode_utf16().collect();
        let end = old
            .len()
            .saturating_sub(new.len().saturating_sub(new_cursor));
        let replacement = String::from_utf16_lossy(new.get(word.start..new_cursor).unwrap_or(&[]));
        self.as_mut().rust_mut().dismissed = None;
        self.as_mut().close_popup();
        self.inserted(
            row(word.start),
            row(end),
            &QString::from(&replacement),
            row(new_cursor),
        );
        true
    }

    fn accept_cursor(self: Pin<&mut Self>) -> bool {
        let at = self.cursor;
        self.accept(at)
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
            (Row::Person(_), ROW_TYPE_ROLE) => text("person"),
            (Row::Special(_), ROW_TYPE_ROLE) => text("special"),
            (Row::More(_), ROW_TYPE_ROLE) => text("more"),
            (Row::More(count), COUNT_ROLE) => {
                QVariant::from(&i32::try_from(*count).unwrap_or(i32::MAX))
            }
            (Row::Person(person), USERNAME_ROLE) => text(&person.username),
            (Row::Special(special), USERNAME_ROLE) => text(special.username),
            (Row::Person(person), NAME_ROLE) => text(person.name.as_deref().unwrap_or_default()),
            (Row::Person(person), NAME_STYLED_ROLE) => text(&styled(
                person.name.as_deref().unwrap_or_default(),
                person.name_match.as_ref(),
            )),
            (Row::Person(person), USERNAME_STYLED_ROLE) => {
                text(&styled(&person.username, person.username_match.as_ref()))
            }
            (Row::Special(special), USERNAME_STYLED_ROLE) => {
                text(&styled(special.username, special.username_match.as_ref()))
            }
            (Row::Special(special), DESCRIPTION_ROLE) => text(special.description),
            (Row::Person(person), AVATAR_ROLE) => text(self.avatars.url(&person.avatar)),
            (Row::Person(person), AVATAR_INITIALS_ROLE) => text(&initials(
                person.name.as_deref().unwrap_or(&person.username),
            )),
            (Row::Person(person), AVATAR_COLOR_ROLE) => text(&color(&person.username)),
            // The delegate reads every role of every row, so a row answers the ones it has no use
            // for with nothing.
            (_, COUNT_ROLE) => QVariant::from(&0_i32),
            (_, USERNAME_ROLE..=DESCRIPTION_ROLE) => text(""),
            _ => QVariant::default(),
        }
    }

    #[allow(clippy::unused_self)] // Signature fixed by `QAbstractItemModel::roleNames`.
    fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut names = QHash::<QHashPair_i32_QByteArray>::default();
        for (role, name) in [
            (ROW_TYPE_ROLE, "rowType"),
            (USERNAME_ROLE, "username"),
            (NAME_ROLE, "name"),
            (NAME_STYLED_ROLE, "nameStyled"),
            (USERNAME_STYLED_ROLE, "usernameStyled"),
            (AVATAR_ROLE, "avatar"),
            (AVATAR_INITIALS_ROLE, "avatarInitials"),
            (AVATAR_COLOR_ROLE, "avatarColor"),
            (COUNT_ROLE, "count"),
            (DESCRIPTION_ROLE, "description"),
        ] {
            names.insert(role, QByteArray::from(name));
        }
        names
    }
}

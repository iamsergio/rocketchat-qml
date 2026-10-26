// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The `RoomSearch` model: what matches what the user typed in the search field of the room list.
//! The user's own rooms, and with them the people and the public channels the server's directory
//! found, in groups (see `rc_view::directory`). Rows are kept bottom to top: the list is laid out
//! from the field up, so row 0 is the one right above it.

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
        // The row the keyboard cursor is on, or -1 for none: always a row that can be acted on.
        // Enter acts on it. A new search puts it on the best match.
        #[qproperty(i32, cursor, READ, NOTIFY)]
        type RoomSearchModel = super::RoomSearchModelRust;

        /// Searches for the rooms matching `query`. A new query is a new set of rows, so the model
        /// resets.
        #[qinvokable]
        #[cxx_name = "setQuery"]
        fn set_query(self: Pin<&mut RoomSearchModel>, query: &QString);

        /// Acts on the row: opens a room, starts a direct message with a person, previews a
        /// channel. Returns whether it did, so that the field knows to clear itself: a label, a
        /// status row, or a person or channel while another directory action is under way, are
        /// not acted on.
        #[qinvokable]
        fn open(self: &RoomSearchModel, row: i32) -> bool;

        /// `open` for the row the cursor is on.
        #[qinvokable]
        #[cxx_name = "openCursor"]
        fn open_cursor(self: &RoomSearchModel) -> bool;

        /// Moves the cursor to the next row up the screen that can be acted on, if there is one.
        #[qinvokable]
        #[cxx_name = "cursorUp"]
        fn cursor_up(self: Pin<&mut RoomSearchModel>);

        /// Moves the cursor to the next row down the screen that can be acted on, if there is
        /// one.
        #[qinvokable]
        #[cxx_name = "cursorDown"]
        fn cursor_down(self: Pin<&mut RoomSearchModel>);

        /// Searches the directory again, after it failed.
        #[qinvokable]
        fn retry(self: &RoomSearchModel);

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
use cxx_qt_lib::{
    QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant, QVector,
};
use rc_core::directory::DirectoryAction;
use rc_core::domain::AvatarOwner;

use crate::hub;
use crate::room_roles::{AVATAR_ROLE, room_data, room_role_names, row};
use rc_view::avatar_view::{AvatarUrls, color, initials, owners, rows_using};
use rc_view::directory::{SearchRow, first_actionable, next_actionable};
use rc_view::room_search::trimmed;

use qobject::RoomSearchModel;

// The roles the rows other than rooms add to those of a room (`room_roles.rs`), whose names they
// share where they mean the same: `name`, `roomId` (of a channel), `avatar` and its placeholder.
const ROW_TYPE_ROLE: i32 = 280;
const GROUP_ROLE: i32 = 281;
const USERNAME_ROLE: i32 = 282;
const MEMBERS_ROLE: i32 = 283;
const COUNT_ROLE: i32 = 284;
const TEXT_ROLE: i32 = 285;
const BUSY_ROLE: i32 = 286;

// The roles of a room that a row of another kind answers too.
const DISPLAY_ROLE: i32 = 0;
const ROOM_ID_ROLE: i32 = 256;
const NAME_ROLE: i32 = 257;
const AVATAR_INITIALS_ROLE: i32 = 264;
const AVATAR_COLOR_ROLE: i32 = 265;

/// Backing data of [`qobject::RoomSearchModel`]: the query, and the rows that match it, as the hub
/// found them.
pub struct RoomSearchModelRust {
    /// The avatars of the results, as they came with them and as they have changed since.
    avatars: AvatarUrls,
    query: String,
    /// Bottom to top.
    rows: Vec<SearchRow>,
    /// The directory action under way, if any: the row it is about says so.
    busy: Option<DirectoryAction>,
    active: bool,
    trimmed_query: QString,
    cursor: i32,
}

impl Default for RoomSearchModelRust {
    fn default() -> Self {
        Self {
            avatars: AvatarUrls::default(),
            query: String::new(),
            rows: Vec::new(),
            busy: None,
            active: false,
            trimmed_query: QString::default(),
            cursor: -1,
        }
    }
}

impl cxx_qt::Initialize for RoomSearchModel {
    fn initialize(self: Pin<&mut Self>) {
        hub::register_room_search(self.qt_thread());
    }
}

/// Whose avatar the row shows, if it shows one.
fn avatar_owner(row: &SearchRow) -> Option<AvatarOwner> {
    match row {
        SearchRow::Room(room) => Some(room.avatar_owner()),
        SearchRow::Person(person) => Some(AvatarOwner::User(person.username.clone())),
        SearchRow::Channel(channel) => Some(AvatarOwner::Room(channel.id.clone())),
        _ => None,
    }
}

impl RoomSearchModel {
    /// Searches again, as the rooms changed or the directory said something: another account's,
    /// or the same one's with a change. Nothing to do unless a search is going on.
    pub fn refresh(self: Pin<&mut Self>) {
        if self.active {
            self.search(true);
        }
    }

    /// The account switched: the new one has not searched the directory for this query.
    pub fn switched(self: Pin<&mut Self>) {
        if self.active {
            hub::search_directory(trimmed(&self.query));
            self.search(false);
        }
    }

    /// Takes the directory action under way in, or none, and announces the rows it changes.
    pub fn set_busy(mut self: Pin<&mut Self>, busy: Option<DirectoryAction>) {
        let changed: Vec<usize> = (0..self.rows.len())
            .filter(|at| {
                let is = |busy: &Option<DirectoryAction>| {
                    busy.as_ref()
                        .is_some_and(|action| self.rows[*at].is_busy_with(action))
                };
                is(&self.busy) != is(&busy)
            })
            .collect();
        self.as_mut().rust_mut().busy = busy;
        let no_parent = QModelIndex::default();
        for at in changed {
            let index = self.index(row(at), 0, &no_parent);
            let mut roles = QVector::<i32>::default();
            roles.append(BUSY_ROLE);
            self.as_mut().data_changed(&index, &index, &roles);
        }
    }

    /// Announces new content in the role of the avatar, and only that, for the rows that show
    /// the new files.
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

    /// Replaces the rows with what matches the query now. A new result set, not a change of the
    /// old one, so the model resets. The cursor stays on its row if the search is the same one
    /// and the row is still there, else it goes to the best match.
    fn search(mut self: Pin<&mut Self>, keep_cursor: bool) {
        let kept = usize::try_from(self.cursor)
            .ok()
            .and_then(|at| self.rows.get(at))
            .filter(|_| keep_cursor)
            .map(SearchRow::key);
        // SAFETY: balanced by `end_reset_model` below.
        unsafe { self.as_mut().begin_reset_model() };
        let found = hub::search_results(&self.query);
        let mut rows = found.rows;
        rows.reverse();
        let at = kept
            .and_then(|key| rows.iter().position(|row| row.key() == key))
            .or_else(|| first_actionable(&rows));
        self.as_mut().rust_mut().rows = rows;
        self.as_mut().rust_mut().avatars = found.avatars;
        self.as_mut().rust_mut().busy = found.busy;
        // SAFETY: after `begin_reset_model`.
        unsafe { self.as_mut().end_reset_model() };
        self.set_cursor(at.map_or(-1, row));
    }

    fn set_cursor(mut self: Pin<&mut Self>, cursor: i32) {
        if self.cursor != cursor {
            self.as_mut().rust_mut().cursor = cursor;
            self.cursor_changed();
        }
    }

    fn set_query(mut self: Pin<&mut Self>, query: &QString) {
        let query = query.to_string();
        let trimmed_query = QString::from(trimmed(&query));
        let active = !trimmed_query.is_empty();
        let same_search = self.trimmed_query == trimmed_query;
        self.as_mut().rust_mut().query = query;
        // Before the search of the rows, so that the directory's answer to the new text finds the
        // rows already waiting for it.
        hub::search_directory(&trimmed_query.to_string());
        self.as_mut().search(same_search);
        if !same_search {
            self.as_mut().rust_mut().trimmed_query = trimmed_query;
            self.as_mut().trimmed_query_changed();
        }
        if self.active != active {
            self.as_mut().rust_mut().active = active;
            self.active_changed();
        }
    }

    fn cursor_up(self: Pin<&mut Self>) {
        self.move_cursor(true);
    }

    fn cursor_down(self: Pin<&mut Self>) {
        self.move_cursor(false);
    }

    fn move_cursor(self: Pin<&mut Self>, up: bool) {
        let Ok(from) = usize::try_from(self.cursor) else {
            return;
        };
        if let Some(to) = next_actionable(&self.rows, from, up) {
            self.set_cursor(row(to));
        }
    }

    fn retry(&self) {
        hub::search_directory(trimmed(&self.query));
    }

    fn open(&self, row: i32) -> bool {
        let Some(found) = usize::try_from(row).ok().and_then(|at| self.rows.get(at)) else {
            return false;
        };
        match found {
            SearchRow::Room(room) => hub::open_room(room.id.to_string()),
            // The core ignores a second directory action, and the field must not clear for it.
            SearchRow::Person(_) | SearchRow::Channel(_) if self.busy.is_some() => return false,
            SearchRow::Person(person) => hub::open_direct(&person.username),
            SearchRow::Channel(channel) => hub::preview_channel(channel.id.to_string()),
            _ => return false,
        }
        true
    }

    fn open_cursor(&self) -> bool {
        self.open(self.cursor)
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
        if role == ROW_TYPE_ROLE {
            return text(match found {
                SearchRow::Room(_) => "room",
                SearchRow::Label(_) => "label",
                SearchRow::Searching => "searching",
                SearchRow::Failed(_) => "failed",
                SearchRow::Person(_) => "person",
                SearchRow::Channel(_) => "channel",
                SearchRow::More(..) => "more",
            });
        }
        if role == BUSY_ROLE {
            let busy = (self.busy.as_ref()).is_some_and(|action| found.is_busy_with(action));
            return QVariant::from(&busy);
        }
        // The delegates of the rows that are not rooms read these three, so every one answers them.
        if !matches!(found, SearchRow::Room(_)) {
            match role {
                GROUP_ROLE => {
                    return text(match found {
                        SearchRow::Label(group) | SearchRow::More(group, _) => group.name(),
                        _ => "",
                    });
                }
                TEXT_ROLE => {
                    return text(match found {
                        SearchRow::Failed(why) => why,
                        _ => "",
                    });
                }
                COUNT_ROLE => {
                    let count = if let SearchRow::More(_, count) = found {
                        *count
                    } else {
                        0
                    };
                    return QVariant::from(&i32::try_from(count).unwrap_or(i32::MAX));
                }
                _ => {}
            }
        }
        match found {
            SearchRow::Room(room) => room_data(room, &self.avatars, role),
            SearchRow::Person(person) => {
                let name = person.name.as_deref().unwrap_or(&person.username);
                match role {
                    DISPLAY_ROLE | NAME_ROLE => text(name),
                    USERNAME_ROLE => text(&person.username),
                    // The delegate of people and channels reads both, so each answers the
                    // other's roles with nothing.
                    ROOM_ID_ROLE => text(""),
                    MEMBERS_ROLE => QVariant::from(&0_i32),
                    AVATAR_ROLE => text(
                        self.avatars
                            .url(&AvatarOwner::User(person.username.clone())),
                    ),
                    AVATAR_INITIALS_ROLE => text(&initials(name)),
                    AVATAR_COLOR_ROLE => text(&color(&person.username)),
                    _ => QVariant::default(),
                }
            }
            SearchRow::Channel(channel) => match role {
                DISPLAY_ROLE | NAME_ROLE => text(&channel.name),
                ROOM_ID_ROLE => text(channel.id.as_str()),
                MEMBERS_ROLE => QVariant::from(&i32::try_from(channel.members).unwrap_or(i32::MAX)),
                USERNAME_ROLE => text(""),
                AVATAR_ROLE => text(self.avatars.url(&AvatarOwner::Room(channel.id.clone()))),
                AVATAR_INITIALS_ROLE => text(&initials(&channel.name)),
                AVATAR_COLOR_ROLE => text(&color(&channel.name)),
                _ => QVariant::default(),
            },
            _ => QVariant::default(),
        }
    }

    #[allow(clippy::unused_self)] // Signature fixed by `QAbstractItemModel::roleNames`.
    fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut names = room_role_names();
        for (role, name) in [
            (ROW_TYPE_ROLE, "rowType"),
            (GROUP_ROLE, "group"),
            (USERNAME_ROLE, "username"),
            (MEMBERS_ROLE, "members"),
            (COUNT_ROLE, "count"),
            (TEXT_ROLE, "text"),
            (BUSY_ROLE, "busy"),
        ] {
            names.insert(role, QByteArray::from(name));
        }
        names
    }
}

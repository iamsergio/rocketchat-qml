// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Element paths, `<window>/<name>`: the single source of truth for the names each frontend gives
//! its elements (`objectName` in QML, `accessible-id` in Slint). See docs/design/gui-testing.md.

pub mod main_window {
    pub const WINDOW: &str = "mainWindow";
    pub const VERSION_LABEL: &str = "mainWindow/versionLabel";

    /// Shown until the core has said whether a stored session resumes.
    pub const STARTUP_BUSY: &str = "mainWindow/startupBusy";

    pub const LOGIN_SERVER_FIELD: &str = "mainWindow/loginServerField";
    pub const LOGIN_USERNAME_FIELD: &str = "mainWindow/loginUsernameField";
    pub const LOGIN_PASSWORD_FIELD: &str = "mainWindow/loginPasswordField";
    pub const LOGIN_BUTTON: &str = "mainWindow/loginButton";
    pub const LOGIN_ERROR: &str = "mainWindow/loginError";
    pub const LOGIN_BUSY: &str = "mainWindow/loginBusy";
    /// The heading of the login page, as its `text`: "Log in to Rocket.Chat", "Add a server" (a
    /// new account while others exist) or "Log in again" (the session of the account ended).
    pub const LOGIN_TITLE: &str = "mainWindow/loginTitle";
    /// The sentence under the heading while adding a server. Visible only then.
    pub const LOGIN_LEDE: &str = "mainWindow/loginLede";
    /// "Your session on <host> has ended...", as its `text`. Visible only while logging in again.
    pub const LOGIN_NOTICE: &str = "mainWindow/loginNotice";
    /// Goes back to the other accounts, forgetting the new one. Visible only while adding a server.
    pub const LOGIN_CANCEL_BUTTON: &str = "mainWindow/loginCancelButton";
    /// Forgets the account whose session ended. Visible only while logging in again.
    pub const LOGIN_REMOVE_SERVER_BUTTON: &str = "mainWindow/loginRemoveServerButton";

    /// The rail of servers at the left of the window. Visible except while the session state is
    /// unknown, at startup.
    pub const SERVER_RAIL: &str = "mainWindow/serverRail";
    /// Adds an account and shows its login page. Hidden while a new account waits for its login:
    /// its tile is where to go for it.
    pub const ADD_SERVER_BUTTON: &str = "mainWindow/addServerButton";

    pub const ROOM_LIST: &str = "mainWindow/roomList";
    /// The section of the favorites in the room list. It is always there, with no height unless
    /// the user has favorites: look at its header for whether it shows, and at its geometry for
    /// where.
    pub const ROOM_SECTION_FAVORITES: &str = "mainWindow/roomSection_favorites";
    /// Its heading ("Favorites"), as its `text`. Visible only while the user has favorites.
    pub const ROOM_SECTION_FAVORITES_HEADER: &str =
        "mainWindow/roomSection_favorites/roomSectionHeader";
    /// The section of the other rooms, like the favorites'.
    pub const ROOM_SECTION_ROOMS: &str = "mainWindow/roomSection_rooms";
    /// Its heading ("Rooms"), as its `text`. Visible only while the user has favorites.
    pub const ROOM_SECTION_ROOMS_HEADER: &str = "mainWindow/roomSection_rooms/roomSectionHeader";
    /// The search field below the room list (Ctrl+K focuses it): its `text` is the query. While it
    /// holds something, `ROOM_SEARCH_RESULTS` is shown in place of the room list.
    pub const ROOM_SEARCH_FIELD: &str = "mainWindow/roomSearchField";
    /// The rooms that match the query, in place of the room list. Visible only while searching; its
    /// `count` is how many match. Its rows are `searchResult_<id>`, see `search_result`.
    pub const ROOM_SEARCH_RESULTS: &str = "mainWindow/roomSearchResults";
    /// Clears the search and leaves the focus in the field. Visible only while it holds something.
    pub const ROOM_SEARCH_CLEAR: &str = "mainWindow/roomSearchClear";
    /// The shortcut ("Ctrl K"). Visible only while the field is empty and hasn't the focus.
    pub const ROOM_SEARCH_HINT: &str = "mainWindow/roomSearchHint";
    /// The toast over the search field, "No new notifications", as its `text`: visible for a
    /// moment after F5 found no held news anywhere.
    pub const NO_NEWS_TOAST: &str = "mainWindow/noNewsToast";
    /// `No rooms match “<query>”`, as its `text`, with the query trimmed. Visible only while
    /// searching, when nothing matches.
    pub const ROOM_SEARCH_EMPTY: &str = "mainWindow/roomSearchEmpty";
    /// The labels over the groups of the search results: "Channels", "People" and "Your rooms", as
    /// their `text`. A group has its label only while it has rows; the rooms' only while the
    /// directory has something above them or is not done.
    pub const DIRECTORY_GROUP_CHANNELS: &str = "mainWindow/directoryGroup_channels";
    pub const DIRECTORY_GROUP_PEOPLE: &str = "mainWindow/directoryGroup_people";
    pub const DIRECTORY_GROUP_ROOMS: &str = "mainWindow/directoryGroup_rooms";
    /// "N more, keep typing", under the last row of the group when the directory has more than it
    /// shows. Visible only then.
    pub const DIRECTORY_MORE_PEOPLE: &str = "mainWindow/directoryMore_people";
    pub const DIRECTORY_MORE_CHANNELS: &str = "mainWindow/directoryMore_channels";
    /// "Searching the directory…". Visible from the typing until the server's answer.
    pub const DIRECTORY_SEARCHING: &str = "mainWindow/directorySearching";
    /// Why the directory could not be searched, as its `text`. Visible only then.
    pub const DIRECTORY_ERROR: &str = "mainWindow/directoryError";
    /// Next to it: searches the directory again.
    pub const DIRECTORY_RETRY: &str = "mainWindow/directoryRetry";
    /// Why the last command on a room (a favorite, mark as read or unread, leaving) failed.
    /// Visible only then, under the room list.
    pub const ROOM_LIST_ERROR: &str = "mainWindow/roomListError";
    /// The reason, as its `text`.
    pub const ROOM_LIST_ERROR_TEXT: &str = "mainWindow/roomListErrorText";
    /// Hides the error.
    pub const ROOM_LIST_ERROR_DISMISS: &str = "mainWindow/roomListErrorDismiss";

    // The menu of a room (right-click on its row) and the confirmation to leave are popups too,
    // addressed from the window while they are open.
    /// An item of the open room menu: "Add to favorites" or "Remove from favorites", as its `text`.
    pub const ROOM_FAVORITE_ACTION: &str = "mainWindow/roomFavoriteAction";
    /// An item of the open room menu: marks the room read. Visible for a room that shows unread.
    pub const ROOM_MARK_READ_ACTION: &str = "mainWindow/roomMarkReadAction";
    /// An item of the open room menu: marks the room unread. Visible for a room that shows read,
    /// except the open room, which stays read.
    pub const ROOM_MARK_UNREAD_ACTION: &str = "mainWindow/roomMarkUnreadAction";
    /// An item of the open room menu: asks for a confirmation to leave the room. Hidden for
    /// direct messages.
    pub const ROOM_LEAVE_ACTION: &str = "mainWindow/roomLeaveAction";
    /// The confirmation to leave a room: the question it asks, naming the room, in its body.
    pub const ROOM_LEAVE_DIALOG: &str = "mainWindow/roomLeaveDialog";
    /// In the confirmation: leaves the room.
    pub const ROOM_LEAVE_CONFIRM: &str = "mainWindow/roomLeaveConfirm";
    /// In the confirmation: stays in the room.
    pub const ROOM_LEAVE_CANCEL: &str = "mainWindow/roomLeaveCancel";
    /// The host of the server on screen, at the top of the sidebar, as its `text`.
    pub const SERVER_NAME: &str = "mainWindow/serverName";
    /// The username there, under the host, as its `text`.
    pub const SERVER_USER: &str = "mainWindow/serverUser";
    /// Opens the settings menu, at the bottom of the rail.
    pub const SETTINGS_BUTTON: &str = "mainWindow/settingsButton";
    /// An item of the open settings menu: logs what helps to diagnose problems (the DPI of the
    /// screens for now) to stderr.
    pub const DUMP_INFO_ACTION: &str = "mainWindow/dumpInfoAction";
    /// The item of the open menu of a server tile that logs out of the account, or removes it once
    /// its login has ended. Open the menu with a right-click on the tile.
    pub const LOGOUT_ACTION: &str = "mainWindow/logoutAction";
    /// The item of the open menu of a right-clicked link (in a message, a quote, an attachment or
    /// the topic) that opens it in the browser.
    pub const LINK_OPEN_ACTION: &str = "mainWindow/linkOpenAction";
    /// The item of that menu that copies the address to the clipboard.
    pub const LINK_COPY_ACTION: &str = "mainWindow/linkCopyAction";
    /// Shown while the realtime connection is down.
    pub const CONNECTION_BANNER: &str = "mainWindow/connectionBanner";

    /// Shown in place of the messages while no room is open.
    pub const ROOM_PLACEHOLDER: &str = "mainWindow/roomPlaceholder";
    /// The name of the open room.
    pub const ROOM_TITLE: &str = "mainWindow/roomTitle";
    /// Who is typing in the open room, as its `text` ("alice is typing…"), between the messages and
    /// the composer; empty for nobody. Visible, with its height taken, whenever a room is open.
    pub const ROOM_TYPING: &str = "mainWindow/roomTyping";
    /// The topic of the open room, under its name, as its `text`: the style sheet and the HTML of
    /// the topic. Visible only while the room has one.
    pub const ROOM_TOPIC: &str = "mainWindow/roomTopic";
    /// Why the messages of the open room couldn't be loaded.
    pub const ROOM_ERROR: &str = "mainWindow/roomError";
    /// The messages of the open room, oldest first. Its `count` is how many are loaded, not how
    /// many are in view.
    pub const MESSAGE_LIST: &str = "mainWindow/messageList";
    /// An invisible element of the Slint app whose `text` is how far the message list is scrolled
    /// up from the newest message, in logical pixels. The Qt app has none: its `ListView` says.
    pub const MESSAGE_LIST_OFFSET: &str = "mainWindow/messageListOffset";
    /// Where, from the left edge of the message list, to press to drag the list by its content:
    /// in the gutter left of the avatars, which no message part uses in either app (nothing
    /// there to select, click or hover over), at half the width of the gutter or less. Down the
    /// list, anywhere that is not under the scroll bar will do.
    pub const MESSAGE_LIST_DRAG_X: f64 = 8.0;
    /// Above the first message once nothing older exists.
    pub const HISTORY_START: &str = "mainWindow/historyStart";
    /// Above the first message while older ones load.
    pub const LOADING_OLDER: &str = "mainWindow/loadingOlder";

    /// The room's box to write in. Its parts are found through it, since the thread pane has a
    /// composer of its own with parts of the same names.
    pub const COMPOSER: &str = "mainWindow/roomComposer";
    /// Where the text to send is typed. Enter sends it, Shift+Enter starts a new line.
    pub const COMPOSER_FIELD: &str = "mainWindow/roomComposer/composerField";
    /// In place of Send while the open room is a channel previewed without being in it: joins it.
    /// Its `text` is "Join", and "Joining…" (disabled) while the server is at it.
    pub const COMPOSER_JOIN: &str = "mainWindow/roomComposer/composerJoin";
    /// Sends the text of the composer, or saves it while a message is edited (the button then
    /// says "Save"). Disabled while it is empty, and while a save is awaited.
    pub const COMPOSER_SEND: &str = "mainWindow/roomComposer/composerSend";
    /// The paperclip at the left of the field: attaches files. Hidden while a message is edited.
    /// The apps take the files named by `RC_TEST_ATTACH_FILES` instead of opening a file dialog.
    pub const COMPOSER_ATTACH: &str = "mainWindow/roomComposer/composerAttach";
    /// Above the field while files wait for Send; the files in it are addressed with
    /// [`staged_file`](super::staged_file).
    pub const COMPOSER_ATTACHMENTS: &str = "mainWindow/roomComposer/composerAttachments";
    /// Above the field while files are being uploaded to the room; the rows in it are addressed
    /// with [`upload`](super::upload).
    pub const COMPOSER_UPLOADS: &str = "mainWindow/roomComposer/composerUploads";
    /// Above the composer while a message is edited.
    pub const COMPOSER_EDIT_BAR: &str = "mainWindow/roomComposer/composerEditBar";
    /// In the bar: stops editing and gives the composer back its draft.
    pub const COMPOSER_EDIT_CANCEL: &str = "mainWindow/roomComposer/composerEditCancel";

    /// Above the composer while a message is quoted.
    pub const COMPOSER_QUOTE_BAR: &str = "mainWindow/roomComposer/composerQuoteBar";
    /// In the bar: the name of the quoted message's author, as its `text`.
    pub const COMPOSER_QUOTE_AUTHOR: &str = "mainWindow/roomComposer/composerQuoteAuthor";
    /// In the bar: the quoted message's text as written, as its `text`. Cut to a line when shown.
    pub const COMPOSER_QUOTE_TEXT: &str = "mainWindow/roomComposer/composerQuoteText";
    /// In the bar: stops quoting.
    pub const COMPOSER_QUOTE_CANCEL: &str = "mainWindow/roomComposer/composerQuoteCancel";

    /// Why the last edit, deletion or reaction failed. Visible only then.
    pub const ROOM_ACTION_ERROR: &str = "mainWindow/roomActionError";
    /// The reason, as its `text`.
    pub const ROOM_ACTION_ERROR_TEXT: &str = "mainWindow/roomActionErrorText";
    /// Hides the error.
    pub const ROOM_ACTION_ERROR_DISMISS: &str = "mainWindow/roomActionErrorDismiss";

    // The menu of a message and the confirmation of a deletion are popups, which hang off the
    // window while they are open and not off the message. Only one is open at a time, so their
    // items are addressed from the window.
    /// An item of the open message menu: quotes the message in the composer. Visible for every
    /// sent text message.
    pub const MESSAGE_QUOTE_ACTION: &str = "mainWindow/msgQuoteAction";
    /// An item of the open message menu: opens the emoji picker to add a reaction. Visible for
    /// every sent text message.
    pub const MESSAGE_REACT_ACTION: &str = "mainWindow/msgReactAction";
    /// An item of the open message menu: edits the message. Visible for the user's own messages.
    pub const MESSAGE_EDIT_ACTION: &str = "mainWindow/msgEditAction";
    /// An item of the open message menu: asks for a confirmation to delete the message.
    pub const MESSAGE_DELETE_ACTION: &str = "mainWindow/msgDeleteAction";
    /// The confirmation to delete a message: the question it asks, in its body.
    pub const DELETE_DIALOG: &str = "mainWindow/deleteDialog";
    /// In the confirmation: deletes the message.
    pub const DELETE_CONFIRM: &str = "mainWindow/deleteConfirm";
    /// In the confirmation: keeps the message.
    pub const DELETE_CANCEL: &str = "mainWindow/deleteCancel";

    // The emoji picker is a popup as well, addressed from the window while it is open. Its emoji
    // are found by name with [`emoji_pick`](super::emoji_pick).
    /// In the open emoji picker: the search box, which has the focus. Its `text` is the query.
    pub const EMOJI_SEARCH: &str = "mainWindow/emojiSearch";
    /// In the open emoji picker: the grid of the emoji that match the query. Only those in view
    /// exist.
    pub const EMOJI_GRID: &str = "mainWindow/emojiGrid";

    // The thread pane, beside the room, visible only while it shows a thread or the room's threads. The elements of its
    // message list are the room's (the parent and the replies also sent to the room are in both),
    // so they are addressed through the pane (see [`thread_message`](super::thread_message)), and
    // the composer through its own name, as in [`THREAD_COMPOSER_FIELD`].
    pub const THREAD_PANE: &str = "mainWindow/threadPane";
    /// "Thread", in the header of the pane while it shows a thread.
    pub const THREAD_TITLE: &str = "mainWindow/threadTitle";
    /// Closes the pane: the thread and the list.
    pub const THREAD_CLOSE: &str = "mainWindow/threadClose";
    /// "Threads", with a chevron, in the header of the pane while it shows a thread: goes back to
    /// the list of threads, opening it if it was not open.
    pub const THREAD_BACK_BUTTON: &str = "mainWindow/threadBackButton";
    /// In the header of the room: shows the pane with the room's threads, or closes the pane if it
    /// shows anything. Its `checked` is not used; the pane's visibility says what it did.
    pub const ROOM_THREADS_BUTTON: &str = "mainWindow/roomThreadsButton";
    /// On the threads button: how many threads of the room have replies the user hasn't read.
    /// Visible only while there are some. Its `text` is `unread` (grey) or `mention` (red: one of
    /// them mentions the user), and the count is the `text` of [`ROOM_THREADS_BADGE_COUNT`].
    pub const ROOM_THREADS_BADGE: &str = "mainWindow/roomThreadsBadge";
    pub const ROOM_THREADS_BADGE_COUNT: &str = "mainWindow/roomThreadsBadgeCount";
    /// "Threads", in the header of the pane while it shows the list.
    pub const THREAD_LIST_TITLE: &str = "mainWindow/threadListTitle";
    /// The list of the room's threads in the pane: visible while the pane shows it, kept (hidden)
    /// under a thread. Escape sent to it goes back from the thread, or closes the list.
    pub const THREAD_LIST: &str = "mainWindow/threadList";
    /// Its rows, the thread with the most recent reply first: the `count` is how many are loaded.
    pub const THREAD_LIST_VIEW: &str = "mainWindow/threadListView";
    /// The segments of the filter: their `checked` says which one is on, and clicking one lists
    /// that kind of thread from the first page.
    pub const THREAD_LIST_FILTER_ALL: &str = "mainWindow/threadListFilterAll";
    pub const THREAD_LIST_FILTER_FOLLOWING: &str = "mainWindow/threadListFilterFollowing";
    /// The threads with replies the user hasn't read. After its label it says how many there are,
    /// in [`THREAD_LIST_FILTER_UNREAD_COUNT`], visible only while there are some.
    pub const THREAD_LIST_FILTER_UNREAD: &str = "mainWindow/threadListFilterUnread";
    pub const THREAD_LIST_FILTER_UNREAD_COUNT: &str = "mainWindow/threadListFilterUnreadCount";
    /// Shown while the first page loads, and under the last row while a later one does.
    pub const THREAD_LIST_LOADING: &str = "mainWindow/threadListLoading";
    pub const THREAD_LIST_LOADING_MORE: &str = "mainWindow/threadListLoadingMore";
    /// Why the list couldn't be loaded, as its `text`, with a button to try again.
    pub const THREAD_LIST_ERROR: &str = "mainWindow/threadListError";
    pub const THREAD_LIST_RETRY: &str = "mainWindow/threadListRetry";
    /// What the filter has nothing of: visible once the list has loaded and holds no thread, with
    /// the words as the `text` of its title and its hint.
    pub const THREAD_LIST_EMPTY: &str = "mainWindow/threadListEmpty";
    pub const THREAD_LIST_EMPTY_TITLE: &str = "mainWindow/threadListEmptyTitle";
    pub const THREAD_LIST_EMPTY_HINT: &str = "mainWindow/threadListEmptyHint";
    /// The messages of the open thread, oldest first: the parent, then the replies. Its `count`
    /// is how many are loaded.
    pub const THREAD_MESSAGE_LIST: &str = "mainWindow/threadMessageList";
    /// The thread pane's counterpart of `MESSAGE_LIST_OFFSET`.
    pub const THREAD_MESSAGE_LIST_OFFSET: &str = "mainWindow/threadMessageListOffset";
    /// Under the parent, once it is there and the thread has replies: "N replies" (or "1 reply"),
    /// as its `text`.
    pub const THREAD_REPLIES: &str = "mainWindow/threadReplies";
    /// Shown in the pane while the newest replies load.
    pub const THREAD_LOADING: &str = "mainWindow/threadLoading";
    /// Why the replies of the open thread couldn't be loaded.
    pub const THREAD_ERROR: &str = "mainWindow/threadError";
    /// Who is typing in the open thread, as its `text` ("alice is typing…"), over the composer;
    /// empty for nobody. Visible, with its height taken, as long as the pane is.
    pub const THREAD_TYPING: &str = "mainWindow/threadTyping";
    /// The box to reply in. Its parts are found through it, as the room's are through [`COMPOSER`].
    pub const THREAD_COMPOSER: &str = "mainWindow/threadComposer";
    /// Where the reply is typed. Enter sends it, Escape goes back to the list of threads when there
    /// is nothing to cancel.
    pub const THREAD_COMPOSER_FIELD: &str = "mainWindow/threadComposer/composerField";
    /// Sends the reply, or saves it while a message is edited.
    pub const THREAD_COMPOSER_SEND: &str = "mainWindow/threadComposer/composerSend";
    /// The thread composer's paperclip, as [`COMPOSER_ATTACH`].
    pub const THREAD_COMPOSER_ATTACH: &str = "mainWindow/threadComposer/composerAttach";
    /// The thread composer's bar of files that wait, as [`COMPOSER_ATTACHMENTS`].
    pub const THREAD_COMPOSER_ATTACHMENTS: &str = "mainWindow/threadComposer/composerAttachments";
    /// The thread composer's bar of uploads, as [`COMPOSER_UPLOADS`].
    pub const THREAD_COMPOSER_UPLOADS: &str = "mainWindow/threadComposer/composerUploads";
    /// The check box under the text: also shows the reply in the room. Its `checked` is whether it
    /// will; it is off for each reply.
    pub const THREAD_ALSO_TO_CHANNEL: &str = "mainWindow/threadComposer/composerAlsoToChannel";
    /// Above the thread composer while a message is quoted.
    pub const THREAD_COMPOSER_QUOTE_BAR: &str = "mainWindow/threadComposer/composerQuoteBar";
    /// Above the thread composer while a message is edited.
    pub const THREAD_COMPOSER_EDIT_BAR: &str = "mainWindow/threadComposer/composerEditBar";
    /// An item of the open message menu: opens the thread of the message. Visible for every sent
    /// text message of the room that is not a reply, and never in the pane.
    pub const MESSAGE_THREAD_ACTION: &str = "mainWindow/msgThreadAction";

    // The indicators of a message the server hasn't confirmed, for a test that can't know its id
    // (the client picks it). They exist only while a message is in that state, so the name alone
    // finds it, as long as there is one such message in view.
    pub const UNSENT_PENDING: &str = "mainWindow/msgPending";
    pub const UNSENT_FAILED: &str = "mainWindow/msgFailed";
    pub const UNSENT_RETRY: &str = "mainWindow/msgRetry";
}

/// The parts of a tile of the rail (see [`server_tile`]).
pub mod server_tile_part {
    /// The tile's letter, as its `text`. Hidden for a new account, which shows a plus.
    pub const MONOGRAM: &str = "serverTileMonogram";
    /// The number of mentions on that server, as its `text`. Visible only while there are some.
    pub const MENTIONS: &str = "serverTileMentions";
    /// The short pip: some room there is unread and the account is not on screen.
    pub const UNREAD: &str = "serverTileUnread";
    /// The warning dot: the login ended.
    pub const EXPIRED: &str = "serverTileExpired";
}

/// The tile of the account at this position in the rail (from 0, in the order they were added), or
/// one of its parts. Pass an empty `part` for the tile itself: it is a button, and its `checked`
/// says whether it is the account on screen.
#[must_use]
pub fn server_tile(index: usize, part: &str) -> String {
    let tile = format!("{}/serverTile_{index}", main_window::WINDOW);
    if part.is_empty() {
        tile
    } else {
        format!("{tile}/{part}")
    }
}

/// The parts of a room row, for [`room`].
pub mod room_row {
    pub const NAME: &str = "roomName";
    /// The icon of a channel.
    pub const KIND_CHANNEL: &str = "roomKind_channel";
    /// The icon of a private group.
    pub const KIND_PRIVATE: &str = "roomKind_private";
    /// The icon of a direct message.
    pub const KIND_DIRECT: &str = "roomKind_direct";
    pub const UNREAD: &str = "roomUnread";
    pub const MENTIONS: &str = "roomMentions";
    /// The avatar, visible once its file has loaded: its `text` is the file URL.
    pub const AVATAR: &str = "roomAvatar";
    /// The initials on a colour, visible until the avatar has loaded, and always for a room
    /// without an avatar of its own.
    pub const AVATAR_PLACEHOLDER: &str = "roomAvatarPlaceholder";
    /// The letters of the placeholder, as its `text`.
    pub const AVATAR_INITIALS: &str = "roomAvatarInitials";
}

/// A part of the row of the room with this id (see [`room_row`]). Rows are addressed by room id
/// because room names aren't unique.
#[must_use]
pub fn room(id: &str, part: &str) -> String {
    format!("{}/room_{id}/{part}", main_window::WINDOW)
}

/// A part of the row of the room with this id in the search results (see [`room_row`]). Named
/// apart from the room list's `room_<id>`, which exists at the same time.
#[must_use]
pub fn search_result(id: &str, part: &str) -> String {
    format!("{}/searchResult_{id}/{part}", main_window::WINDOW)
}

/// The parts of the row of a person or a channel the directory found, for [`directory_person`]
/// and [`directory_channel`].
pub mod directory_row {
    /// The display name, or the username of a person with no other name.
    pub const NAME: &str = "directoryName";
    /// The username after the name, faint. Visible only for a person whose name is another.
    pub const USERNAME: &str = "directoryUsername";
    /// What a click does ("Message" for a person, "View" for a channel). Visible only while the
    /// pointer or the keyboard cursor is on the row and nothing is under way for it.
    pub const HINT: &str = "directoryHint";
    /// "N members" of a channel, in the place of the hint when it is not shown.
    pub const MEMBERS: &str = "directoryMembers";
    /// "Opening…", visible while the server is at it, in place of the hint.
    pub const BUSY: &str = "directoryBusy";
}

/// A part of the row of the person with this username in the search results (see
/// [`directory_row`]).
#[must_use]
pub fn directory_person(username: &str, part: &str) -> String {
    format!("{}/directoryPerson_{username}/{part}", main_window::WINDOW)
}

/// A part of the row of the channel with this id in the search results (see [`directory_row`]).
#[must_use]
pub fn directory_channel(id: &str, part: &str) -> String {
    format!("{}/directoryChannel_{id}/{part}", main_window::WINDOW)
}

/// The parts of a message row, for [`message`].
pub mod message_row {
    /// The heading of the day, on the first message of each day.
    pub const DAY_SEPARATOR: &str = "msgDaySeparator";
    pub const AUTHOR: &str = "msgAuthor";
    /// The author's avatar, visible once its file has loaded: its `text` is the file URL.
    pub const AVATAR: &str = "msgAvatar";
    /// The initials on a colour, visible until the avatar has loaded.
    pub const AVATAR_PLACEHOLDER: &str = "msgAvatarPlaceholder";
    /// The letters of the placeholder, as its `text`.
    pub const AVATAR_INITIALS: &str = "msgAvatarInitials";
    pub const TIME: &str = "msgTime";
    /// The "(edited)" marker next to the time: visible only for a message that was edited.
    pub const EDITED: &str = "msgEdited";
    /// The message as written: its `text` is the markdown source. Double-clicking it opens the
    /// emoji picker to react to the message, if it can be reacted to.
    pub const BODY: &str = "msgBody";
    /// The message as shown: its `text` is the rich text (HTML) of the body.
    pub const BODY_HTML: &str = "msgBodyHtml";
    /// Visible when the message mentions the user (by name, `@all` or `@here`).
    pub const MENTIONS_ME: &str = "msgMentionsMe";
    /// What a system message says. These have it instead of an author and a body.
    pub const SYSTEM: &str = "msgSystem";
    /// The chips of the reactions, visible if there are any. Each is a
    /// [`reaction`](super::reaction).
    pub const REACTIONS: &str = "msgReactions";
    /// The "+" after the chips: opens the emoji picker. Visible when the message has reactions and
    /// can be reacted to; it is clear until the pointer is over the message, but a click gets to it
    /// all the same.
    pub const REACTION_ADD: &str = "reactionAdd";
    /// Opens the message's menu (quote, add reaction, edit, delete). Visible only for a message
    /// that can be quoted or reacted to or that the user can change; it is clear until the pointer
    /// is over the message, but a click gets to it all the same.
    pub const MENU_BUTTON: &str = "msgMenuButton";
    /// Exists while the message is being sent.
    pub const PENDING: &str = "msgPending";
    /// Exists when the message couldn't be sent.
    pub const FAILED: &str = "msgFailed";
    /// Exists with `FAILED`: sends the message again.
    pub const RETRY: &str = "msgRetry";
    /// Under a message that has replies, in the room: opens the thread. Visible only then.
    pub const THREAD_SUMMARY: &str = "msgThreadSummary";
    /// In the summary: "N replies" (or "1 reply"), as its `text`.
    pub const THREAD_COUNT: &str = "msgThreadCount";
    /// In the summary: "Last reply" and the time of day of the last reply, or its day if it is not
    /// today, as its `text`.
    pub const THREAD_LAST_REPLY: &str = "msgThreadLastReply";
    /// Marks the message whose thread is open: visible only then, and only in the room.
    pub const THREAD_OPEN: &str = "msgThreadOpen";
    /// Above a reply that was also sent to the room, in the room: "Replied in a thread", and the
    /// start of the parent's text when the room holds it, as its `text`. Clicking opens the thread.
    pub const IN_THREAD: &str = "msgInThread";
}

/// A part of the row of the message with this id (see [`message_row`]). Like rooms, messages are
/// addressed by id: their texts repeat.
#[must_use]
pub fn message(id: &str, part: &str) -> String {
    format!("{}/msg_{id}/{part}", main_window::WINDOW)
}

/// A part of the row, in the thread pane, of the message with this id (see [`message_row`]): the
/// parent and the replies, which the room may show too under the same names.
#[must_use]
pub fn thread_message(id: &str, part: &str) -> String {
    format!("{}/threadPane/msg_{id}/{part}", main_window::WINDOW)
}

/// The parts of a row of the thread list, for [`thread_list_row`].
pub mod thread_list_row_part {
    pub const AUTHOR: &str = "threadListAuthor";
    /// The time of the last reply, or the heading of its day, as its `text`.
    pub const TIME: &str = "threadListTime";
    /// The start of the parent's text, on at most two lines: its `text` is all it was given.
    pub const TEXT: &str = "threadListText";
    /// "N replies" (or "1 reply"), as its `text`.
    pub const SUMMARY: &str = "threadListSummary";
    /// "New", visible while the thread has replies the user hasn't read. The server says which
    /// threads are unread, not how many replies.
    pub const NEW: &str = "threadListNew";
    /// "@", visible while one of those mentions the user.
    pub const MENTION: &str = "threadListMention";
    /// The author's avatar, visible once its file has loaded: its `text` is the file URL.
    pub const AVATAR: &str = "threadListAvatar";
    /// The initials on a colour, visible until the avatar has loaded.
    pub const AVATAR_PLACEHOLDER: &str = "threadListAvatarPlaceholder";
    /// The letters of the placeholder, as its `text`.
    pub const AVATAR_INITIALS: &str = "threadListAvatarInitials";
}

/// The row of the thread whose parent has this id in the thread list, or a part of it (see
/// [`thread_list_row_part`]); an empty `part` is the row itself, which can be clicked to open the
/// thread. Rows are addressed by the parent's id, as messages are: their texts repeat.
#[must_use]
pub fn thread_list_row(id: &str, part: &str) -> String {
    let row = format!("{}/threadListRow_{id}", main_window::WINDOW);
    if part.is_empty() {
        row
    } else {
        format!("{row}/{part}")
    }
}

/// The parts of an attachment of a message, for [`attachment`].
pub mod attachment_row {
    /// The picture of an image, visible once its file has loaded: its `text` is the file URL.
    pub const IMAGE: &str = "attachmentImage";
    /// The grey box of an image's size, visible until the picture has loaded.
    pub const PLACEHOLDER: &str = "attachmentPlaceholder";
    /// A file that isn't an image, as a link: visible for files only, its `text` is the address it
    /// opens.
    pub const LINK: &str = "attachmentLink";
    /// The file's name, in the link.
    pub const TITLE: &str = "attachmentTitle";
    /// The file's size in words (`1.5 KB`), in the link.
    pub const SIZE: &str = "attachmentSize";
    /// What the uploader wrote about the file; visible only if they did.
    pub const DESCRIPTION: &str = "attachmentDescription";
}

/// A part of the attachment with this index (0 is the first) of the message with this id (see
/// [`attachment_row`]). The attachments of a message are named by their index.
#[must_use]
pub fn attachment(message_id: &str, index: usize, part: &str) -> String {
    format!(
        "{}/msg_{message_id}/attachment{index}/{part}",
        main_window::WINDOW
    )
}

/// The parts of a quote in a message, for [`quote`].
pub mod quote_row {
    /// The username of the quoted message's author: its `text`.
    pub const AUTHOR: &str = "quoteAuthor";
    /// The time of day of the quoted message: its `text`. Empty if the server didn't say.
    pub const TIME: &str = "quoteTime";
    /// The quoted message as written, without the links of what it quotes itself: its `text` is
    /// the markdown source.
    pub const BODY: &str = "quoteBody";
    /// The quoted message as shown: its `text` is the rich text (HTML) of the body.
    pub const BODY_HTML: &str = "quoteBodyHtml";
}

/// A part of the quote with this index (0 is the first) of the message with this id (see
/// [`quote_row`]), or the quote itself with an empty `part`. The quotes of a message are named by
/// their index, and exist only while the message has them.
#[must_use]
pub fn quote(message_id: &str, index: usize, part: &str) -> String {
    let quote = format!("{}/msg_{message_id}/quote{index}", main_window::WINDOW);
    if part.is_empty() {
        quote
    } else {
        format!("{quote}/{part}")
    }
}

/// The parts of a file that waits in a composer, for [`staged_file`].
pub mod staged_file_part {
    /// The file's name: its `text`.
    pub const NAME: &str = "composerAttachmentName";
    /// The file's size in words (`1.5 KB`): its `text`.
    pub const SIZE: &str = "composerAttachmentSize";
    /// The picture itself, for an image file: visible only then.
    pub const THUMBNAIL: &str = "thumbnailImage";
}

/// The parts of the mention popup above a composer, for [`mention`].
pub mod mention_part {
    /// The popup itself: visible while it is open.
    pub const POPUP: &str = "mentionPopup";
    /// Its label, as its `text`: "Members", or "In this conversation" in a direct message (shown in
    /// capitals).
    pub const LABEL: &str = "mentionLabel";
    /// "Searching members…", visible while the search runs and there is nothing to show yet.
    pub const SEARCHING: &str = "mentionSearching";
    /// Why the members could not be searched, as its `text`. Visible only then.
    pub const ERROR: &str = "mentionError";
    /// "No member matches “@query”.", visible when nothing is listed and the search is done.
    pub const NONE: &str = "mentionNone";
    /// "N more, keep typing", as its `text`.
    pub const MORE: &str = "mentionMore";
    /// The hint line with the keys.
    pub const HINT: &str = "mentionHint";
}

/// A part of the mention popup above the composer `composer` (`main_window::COMPOSER` or
/// `main_window::THREAD_COMPOSER`), see [`mention_part`].
#[must_use]
pub fn mention(composer: &str, part: &str) -> String {
    format!("{composer}/{part}")
}

/// The parts of a row of the mention popup, for [`mention_row`].
pub mod mention_row_part {
    /// The display name, visible for a person who has one.
    pub const NAME: &str = "mentionName";
    /// `@username` of a person, as styled text.
    pub const USERNAME: &str = "mentionUsername";
    /// `@all` or `@here` as styled text, in the place of the name.
    pub const SPECIAL_NAME: &str = "mentionSpecialName";
    /// What `@all` or `@here` does, as its `text`.
    pub const DESCRIPTION: &str = "mentionDescription";
}

/// The row of the person with this username (`all` or `here` for the two groups) in the popup of
/// the composer `composer`, or a part of it (see [`mention_row_part`]; an empty `part` is the row
/// itself, whose `text` is the username).
#[must_use]
pub fn mention_row(composer: &str, username: &str, part: &str) -> String {
    let row = format!("{composer}/mentionRow_{username}");
    if part.is_empty() {
        row
    } else {
        format!("{row}/{part}")
    }
}

/// The file with this index (0 is the first, in the order attached) that waits in the composer
/// `composer` (`main_window::COMPOSER` or `main_window::THREAD_COMPOSER`), or a part of it (see
/// [`staged_file_part`]; an empty `part` is the chip itself).
#[must_use]
pub fn staged_file(composer: &str, index: usize, part: &str) -> String {
    let chip = format!("{composer}/composerAttachment{index}");
    if part.is_empty() {
        chip
    } else {
        format!("{chip}/{part}")
    }
}

/// The cross of the file with this index in the composer `composer`: takes it out.
#[must_use]
pub fn staged_file_remove(composer: &str, index: usize) -> String {
    format!("{composer}/composerAttachmentRemove{index}")
}

/// The parts of an upload under way in a composer, for [`upload`].
pub mod upload_part {
    /// The file's name: its `text`.
    pub const NAME: &str = "uploadName";
    /// What it is doing, as its `text`: `1.8 of 4.2 MB`, `Waiting · 4.2 MB`, `Posting…`, or why it
    /// failed.
    pub const STATUS: &str = "uploadStatus";
    /// The track of the progress: visible except for a failed upload.
    pub const PROGRESS: &str = "uploadProgress";
}

/// The upload with this index (0 is the first, in the order asked) in the bar of the composer
/// `composer`, or a part of it (see [`upload_part`]; an empty `part` is the row itself). Rows
/// exist while the upload is listed.
#[must_use]
pub fn upload(composer: &str, index: usize, part: &str) -> String {
    let row = format!("{composer}/upload{index}");
    if part.is_empty() {
        row
    } else {
        format!("{row}/{part}")
    }
}

/// The button of the upload with this index in the composer `composer` that gives it up: a cross
/// while it waits or is on its way, "Remove" once it failed. Not there while its message is posted.
#[must_use]
pub fn upload_cancel(composer: &str, index: usize) -> String {
    format!("{composer}/uploadCancel{index}")
}

/// The "Retry" button of the failed upload with this index in the composer `composer`.
#[must_use]
pub fn upload_retry(composer: &str, index: usize) -> String {
    format!("{composer}/uploadRetry{index}")
}

/// An emoji of the open emoji picker, by its shortcode without the colons (`tada` for `:tada:`).
/// It exists while the grid shows it.
#[must_use]
pub fn emoji_pick(name: &str) -> String {
    format!("{}/emojiPick_{name}", main_window::WINDOW)
}

/// The parts of a reaction of a message, for [`reaction`].
pub mod reaction_row {
    /// The emoji: its `text` is the Unicode character, else the address of the custom emoji's
    /// image, else the shortcode.
    pub const EMOJI: &str = "reactionEmoji";
    /// How many users reacted: its `text` is the number.
    pub const COUNT: &str = "reactionCount";
}

/// A part of the reaction with this index (0 is the first, in the server's order) of the message
/// with this id (see [`reaction_row`]), or the chip itself with an empty `part`, whose `checked`
/// says whether the user is one of those who reacted.
#[must_use]
pub fn reaction(message_id: &str, index: usize, part: &str) -> String {
    let chip = format!("{}/msg_{message_id}/reaction{index}", main_window::WINDOW);
    if part.is_empty() {
        chip
    } else {
        format!("{chip}/{part}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // INFRA-002
    #[test]
    fn a_room_part_is_addressed_by_the_room_id() {
        assert_eq!(
            room("abc123", room_row::UNREAD),
            "mainWindow/room_abc123/roomUnread"
        );
    }

    // INFRA-002
    #[test]
    fn a_search_result_part_is_addressed_by_the_room_id() {
        assert_eq!(
            search_result("abc123", room_row::NAME),
            "mainWindow/searchResult_abc123/roomName"
        );
    }

    // INFRA-002
    #[test]
    fn a_directory_row_part_is_addressed_by_the_username_or_the_room_id() {
        assert_eq!(
            directory_person("bob", directory_row::HINT),
            "mainWindow/directoryPerson_bob/directoryHint"
        );
        assert_eq!(
            directory_channel("c1", directory_row::MEMBERS),
            "mainWindow/directoryChannel_c1/directoryMembers"
        );
    }

    // INFRA-002
    #[test]
    fn a_thread_list_row_and_its_parts_are_addressed_by_the_parents_id() {
        assert_eq!(thread_list_row("m7", ""), "mainWindow/threadListRow_m7");
        assert_eq!(
            thread_list_row("m7", thread_list_row_part::SUMMARY),
            "mainWindow/threadListRow_m7/threadListSummary"
        );
    }

    // INFRA-002
    #[test]
    fn a_message_part_is_addressed_by_the_message_id() {
        assert_eq!(
            message("m7", message_row::BODY),
            "mainWindow/msg_m7/msgBody"
        );
        assert_eq!(
            message("m7", message_row::DAY_SEPARATOR),
            "mainWindow/msg_m7/msgDaySeparator"
        );
    }

    // INFRA-002
    #[test]
    fn a_thread_message_part_is_addressed_through_the_pane() {
        assert_eq!(
            thread_message("m7", message_row::BODY),
            "mainWindow/threadPane/msg_m7/msgBody"
        );
    }

    // INFRA-002
    #[test]
    fn an_attachment_part_is_addressed_by_the_message_id_and_the_index() {
        assert_eq!(
            attachment("m7", 1, attachment_row::LINK),
            "mainWindow/msg_m7/attachment1/attachmentLink"
        );
    }

    // INFRA-002
    #[test]
    fn a_quote_part_is_addressed_by_the_message_id_and_the_index() {
        assert_eq!(
            quote("m7", 0, quote_row::AUTHOR),
            "mainWindow/msg_m7/quote0/quoteAuthor"
        );
        assert_eq!(quote("m7", 1, ""), "mainWindow/msg_m7/quote1");
    }

    // INFRA-002
    #[test]
    fn mention_popup_parts_are_addressed_through_their_composer() {
        assert_eq!(
            mention(main_window::THREAD_COMPOSER, mention_part::HINT),
            "mainWindow/threadComposer/mentionHint"
        );
        assert_eq!(
            mention_row(main_window::COMPOSER, "ann", mention_row_part::NAME),
            "mainWindow/roomComposer/mentionRow_ann/mentionName"
        );
        assert_eq!(
            mention_row(main_window::COMPOSER, "ann", ""),
            "mainWindow/roomComposer/mentionRow_ann"
        );
    }

    // INFRA-002
    #[test]
    fn staged_files_and_uploads_are_addressed_through_their_composer() {
        assert_eq!(
            staged_file(main_window::COMPOSER, 1, staged_file_part::SIZE),
            "mainWindow/roomComposer/composerAttachment1/composerAttachmentSize"
        );
        assert_eq!(
            staged_file_remove(main_window::THREAD_COMPOSER, 0),
            "mainWindow/threadComposer/composerAttachmentRemove0"
        );
        assert_eq!(
            upload(main_window::COMPOSER, 0, ""),
            "mainWindow/roomComposer/upload0"
        );
        assert_eq!(
            upload(main_window::COMPOSER, 2, upload_part::STATUS),
            "mainWindow/roomComposer/upload2/uploadStatus"
        );
        assert_eq!(
            upload_retry(main_window::COMPOSER, 0),
            "mainWindow/roomComposer/uploadRetry0"
        );
        assert_eq!(
            upload_cancel(main_window::THREAD_COMPOSER, 3),
            "mainWindow/threadComposer/uploadCancel3"
        );
    }

    // INFRA-002
    #[test]
    fn an_emoji_of_the_picker_is_addressed_by_its_name_from_the_window() {
        assert_eq!(emoji_pick("tada"), "mainWindow/emojiPick_tada");
    }

    // INFRA-002
    #[test]
    fn a_reaction_part_is_addressed_by_the_message_id_and_the_index() {
        assert_eq!(
            reaction("m7", 2, reaction_row::COUNT),
            "mainWindow/msg_m7/reaction2/reactionCount"
        );
        assert_eq!(reaction("m7", 0, ""), "mainWindow/msg_m7/reaction0");
    }
}

/// The GitHub page: its tile in the rail, the connect page, the list of pull requests and the page
/// of the one that is open. See docs/design/gui-testing.md.
pub mod github {
    use super::main_window::WINDOW;

    /// The tile at the bottom of the rail, above the settings: a button whose `checked` says that
    /// the GitHub page is on screen. Its parts are `githubTileFailing` (its `text` is how many of
    /// the user's pull requests fail; visible only while connected and above zero),
    /// `githubTilePip` (the short pip: a pull request finished while the page wasn't looked at)
    /// and `githubTileCurrent` (the tall pip: the page is on screen).
    pub const TILE: &str = "mainWindow/githubTile";
    pub const TILE_FAILING: &str = "mainWindow/githubTile/githubTileFailing";
    pub const TILE_PIP: &str = "mainWindow/githubTile/githubTilePip";
    pub const TILE_CURRENT: &str = "mainWindow/githubTile/githubTileCurrent";
    /// The item of the tile's right-click menu, a popup that exists only while the menu is open
    /// (and only for a tile that is connected).
    pub const DISCONNECT_ACTION: &str = "mainWindow/githubDisconnectAction";

    /// The connect page. Visible while GitHub is not connected and its page is on screen.
    pub const CONNECT_TITLE: &str = "mainWindow/githubConnectTitle";
    /// "Use it": connects with the login of the gh command.
    pub const CONNECT_GH: &str = "mainWindow/githubConnectGh";
    /// The field for a personal access token: its `text` is the token.
    pub const CONNECT_TOKEN_FIELD: &str = "mainWindow/githubConnectTokenField";
    pub const CONNECT_TOKEN: &str = "mainWindow/githubConnectToken";
    /// The spinner, visible while connecting.
    pub const CONNECT_BUSY: &str = "mainWindow/githubConnectBusy";
    /// Why connecting failed, as its `text`. Visible only then.
    pub const CONNECT_ERROR: &str = "mainWindow/githubConnectError";

    /// The header of the list: its `text` is "Pull requests".
    pub const TITLE: &str = "mainWindow/githubTitle";
    /// "<login>, updated 40 s ago", "updating…" or why the last fetch failed, as its `text`.
    pub const STATUS: &str = "mainWindow/githubStatus";
    pub const REFRESH_BUTTON: &str = "mainWindow/githubRefreshButton";
    /// The two tabs: their `checked` says which list shows. The counts are the `text` of
    /// `githubTabMineCount` and `githubTabWatchedCount`, below them.
    pub const TAB_MINE: &str = "mainWindow/githubTabMine";
    pub const TAB_WATCHED: &str = "mainWindow/githubTabWatched";
    pub const TAB_MINE_COUNT: &str = "mainWindow/githubTabMine/githubTabMineCount";
    pub const TAB_WATCHED_COUNT: &str = "mainWindow/githubTabWatched/githubTabWatchedCount";
    /// The list: its `count` is the rows, the heading of the drafts included.
    pub const LIST: &str = "mainWindow/githubPrList";
    /// Visible while the list has no rows.
    pub const EMPTY: &str = "mainWindow/githubEmpty";
    /// The heading that folds the drafts: its `checked` says they show.
    pub const DRAFTS: &str = "mainWindow/githubDrafts";
    /// Its text, "Drafts 4".
    pub const DRAFTS_TITLE: &str = "mainWindow/githubDrafts/githubDraftsTitle";

    /// The row at this position of the list (from 0, the heading of the drafts counting as a row),
    /// or one of its parts: the tile of a pull request is a button whose `checked` says its page is
    /// open.
    #[must_use]
    pub fn pr_row(index: usize, part: &str) -> String {
        let row = format!("{WINDOW}/prRow{index}");
        if part.is_empty() {
            row
        } else {
            format!("{row}/{part}")
        }
    }

    /// The parts of a row, for [`pr_row`].
    pub mod pr_row_part {
        /// The state: its `text` is `draft`, `failed`, `waiting`, `running`, `passed` or `none`.
        pub const GLYPH: &str = "prGlyph";
        /// The repository, and on the Watched tab ", <author>"; its `text`.
        pub const REPO: &str = "prRepo";
        /// "40 s", "11 min": its `text`.
        pub const AGE: &str = "prAge";
        /// Visible while the user asked to be told when its CI ends.
        pub const BELL: &str = "prBell";
        /// The title as styled text, then the number, as its `text`.
        pub const TITLE: &str = "prTitle";
        /// The bar of the jobs: its `visible` is whether there are any.
        pub const BAR: &str = "prBar";
    }

    /// Segment `index` of the check bar of the row: its `text` is `failed`, `running`,
    /// `cancelled`, `passed` or `waiting`.
    #[must_use]
    pub fn pr_segment(row: usize, index: usize) -> String {
        format!("{WINDOW}/prRow{row}/prBar/prSegment{index}")
    }

    /// The runner at this position of the foot of the list, or its part (`githubRunnerName`,
    /// `githubRunnerStatus`, `githubRunnerLamp`).
    #[must_use]
    pub fn runner(index: usize, part: &str) -> String {
        let runner = format!("{WINDOW}/githubRunner{index}");
        if part.is_empty() {
            runner
        } else {
            format!("{runner}/{part}")
        }
    }

    /// "Pick a pull request to see its checks.", visible while none is open.
    pub const PICK_HINT: &str = "mainWindow/githubPickHint";
    /// The header of the open pull request, each part by its `text`: `prPageRepo`, `prPageNumber`
    /// ("#41"), `prPageUpdated` ("updated 3 min ago"), `prPageTitle`, `prPageHead` and `prPageBase`
    /// (the branches), `prPageReview` and `prPageBaseNote` (visible when they have something to
    /// say), `prPageDraft` (visible for a draft) and the button `prPageOpenButton`.
    pub const PAGE_REPO: &str = "mainWindow/prPageRepo";
    pub const PAGE_NUMBER: &str = "mainWindow/prPageNumber";
    pub const PAGE_UPDATED: &str = "mainWindow/prPageUpdated";
    pub const PAGE_TITLE: &str = "mainWindow/prPageTitle";
    pub const PAGE_HEAD: &str = "mainWindow/prPageHead";
    pub const PAGE_BASE: &str = "mainWindow/prPageBase";
    pub const PAGE_REVIEW: &str = "mainWindow/prPageReview";
    pub const PAGE_BASE_NOTE: &str = "mainWindow/prPageBaseNote";
    pub const PAGE_DRAFT: &str = "mainWindow/prPageDraft";
    pub const PAGE_OPEN_BUTTON: &str = "mainWindow/prPageOpenButton";

    /// The banner over the actions, visible only while runs wait for approval, its `text`
    /// `prApprovalText` ("Runs are waiting for your approval") and its button `prApprove`.
    pub const APPROVAL: &str = "mainWindow/prApproval";
    pub const APPROVAL_TEXT: &str = "mainWindow/prApprovalText";
    pub const APPROVE: &str = "mainWindow/prApprove";
    /// The buttons of the actions: their `enabled` and `visible` follow the view; `prMergeReason`
    /// (its `text`) says why Merge is disabled and is visible only then.
    pub const MERGE: &str = "mainWindow/prMerge";
    pub const MERGE_REASON: &str = "mainWindow/prMergeReason";
    pub const MERGE_ANYWAY: &str = "mainWindow/prMergeAnyway";
    pub const UPDATE_BRANCH: &str = "mainWindow/prUpdateBranch";
    pub const RERUN_FAILED: &str = "mainWindow/prRerunFailed";
    /// The notify toggle: its `text` is "Notify me when done" or "Notifying you".
    pub const NOTIFY: &str = "mainWindow/prNotify";
    /// The question of Merge, in place of the buttons: its `text` (visible only then), with
    /// `prConfirmMerge` or `prConfirmMergeAnyway` to say yes and `prConfirmCancel` to say no.
    pub const CONFIRM: &str = "mainWindow/prConfirm";
    pub const CONFIRM_MERGE: &str = "mainWindow/prConfirmMerge";
    pub const CONFIRM_MERGE_ANYWAY: &str = "mainWindow/prConfirmMergeAnyway";
    pub const CONFIRM_CANCEL: &str = "mainWindow/prConfirmCancel";
    /// Why the last command failed, as its `text` (visible only then), and the button that
    /// closes it.
    pub const ERROR: &str = "mainWindow/prError";
    pub const ERROR_DISMISS: &str = "mainWindow/prErrorDismiss";
    /// What the last command did, as its `text`; visible for a moment.
    pub const TOAST: &str = "mainWindow/prToast";
    /// The checks: the list that scrolls, the heading (`text`), the tally (`text` "1 failed, 2
    /// running") and the bar, whose segment `i` is [`checks_segment`].
    pub const CHECKS: &str = "mainWindow/prChecks";
    pub const CHECKS_HEADING: &str = "mainWindow/prChecksHeading";
    pub const TALLY: &str = "mainWindow/prTally";
    pub const CHECKS_BAR: &str = "mainWindow/prChecksBar";

    /// Segment `index` of the bar of the page: its `text` is `failed`, `running`, `cancelled`,
    /// `passed` or `waiting`.
    #[must_use]
    pub fn checks_segment(index: usize) -> String {
        format!("{WINDOW}/prChecksBar/prSegment{index}")
    }

    /// The heading of a group of jobs, `Failed`, `Running`, `Cancelled` or `Passed`: its
    /// `checked` says the group is open; `prJobGroupTitle` below it has its `text` ("Failed 2").
    #[must_use]
    pub fn job_group(name: &str) -> String {
        format!("{WINDOW}/prJobGroup{name}")
    }

    /// The title of the group `name`.
    #[must_use]
    pub fn job_group_title(name: &str) -> String {
        format!("{WINDOW}/prJobGroup{name}/prJobGroupTitle")
    }

    /// The row of the job at this position of the bar (it exists while its group is open), or one
    /// of its parts: `prJobState` (`failed`, `running`, `cancelled` or `passed`), `prJobWorkflow`
    /// ("CI / "), `prJobName` and `prJobDuration`, by their `text`.
    #[must_use]
    pub fn job(index: usize, part: &str) -> String {
        let job = format!("{WINDOW}/prJob{index}");
        if part.is_empty() {
            job
        } else {
            format!("{job}/{part}")
        }
    }

    /// The Rerun button of the job at this position of the bar, visible for failed jobs.
    #[must_use]
    pub fn job_rerun(index: usize) -> String {
        format!("{WINDOW}/prJobRerun{index}")
    }
}

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
    /// `No rooms match “<query>”`, as its `text`, with the query trimmed. Visible only while
    /// searching, when nothing matches.
    pub const ROOM_SEARCH_EMPTY: &str = "mainWindow/roomSearchEmpty";
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
    /// Above the first message once nothing older exists.
    pub const HISTORY_START: &str = "mainWindow/historyStart";
    /// Above the first message while older ones load.
    pub const LOADING_OLDER: &str = "mainWindow/loadingOlder";

    /// Where the text to send is typed. Enter sends it, Shift+Enter starts a new line.
    pub const COMPOSER_FIELD: &str = "mainWindow/composerField";
    /// Sends the text of the composer, or saves it while a message is edited (the button then
    /// says "Save"). Disabled while it is empty, and while a save is awaited.
    pub const COMPOSER_SEND: &str = "mainWindow/composerSend";
    /// Above the composer while a message is edited.
    pub const COMPOSER_EDIT_BAR: &str = "mainWindow/composerEditBar";
    /// In the bar: stops editing and gives the composer back its draft.
    pub const COMPOSER_EDIT_CANCEL: &str = "mainWindow/composerEditCancel";

    /// Above the composer while a message is quoted.
    pub const COMPOSER_QUOTE_BAR: &str = "mainWindow/composerQuoteBar";
    /// In the bar: the name of the quoted message's author, as its `text`.
    pub const COMPOSER_QUOTE_AUTHOR: &str = "mainWindow/composerQuoteAuthor";
    /// In the bar: the quoted message's text as written, as its `text`. Cut to a line when shown.
    pub const COMPOSER_QUOTE_TEXT: &str = "mainWindow/composerQuoteText";
    /// In the bar: stops quoting.
    pub const COMPOSER_QUOTE_CANCEL: &str = "mainWindow/composerQuoteCancel";

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

    // The thread pane, beside the room, visible only while a thread is open. The elements of its
    // message list are the room's (the parent and the replies also sent to the room are in both),
    // so they are addressed through the pane (see [`thread_message`](super::thread_message)), and
    // the composer through its own name, as in [`THREAD_COMPOSER_FIELD`].
    pub const THREAD_PANE: &str = "mainWindow/threadPane";
    /// "Thread", in the header of the pane.
    pub const THREAD_TITLE: &str = "mainWindow/threadTitle";
    /// Closes the thread.
    pub const THREAD_CLOSE: &str = "mainWindow/threadClose";
    /// The messages of the open thread, oldest first: the parent, then the replies. Its `count`
    /// is how many are loaded.
    pub const THREAD_MESSAGE_LIST: &str = "mainWindow/threadMessageList";
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
    /// The box to reply in. Its parts are found through it, as the room's are by name alone.
    pub const THREAD_COMPOSER: &str = "mainWindow/threadComposer";
    /// Where the reply is typed. Enter sends it, Escape closes the thread when there is nothing to
    /// cancel.
    pub const THREAD_COMPOSER_FIELD: &str = "mainWindow/threadComposer/composerField";
    /// Sends the reply, or saves it while a message is edited.
    pub const THREAD_COMPOSER_SEND: &str = "mainWindow/threadComposer/composerSend";
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

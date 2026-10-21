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

    pub const ROOM_LIST: &str = "mainWindow/roomList";
    pub const CURRENT_USER: &str = "mainWindow/currentUser";
    pub const LOGOUT_BUTTON: &str = "mainWindow/logoutButton";
    /// Shown while the realtime connection is down.
    pub const CONNECTION_BANNER: &str = "mainWindow/connectionBanner";

    /// Shown in place of the messages while no room is open.
    pub const ROOM_PLACEHOLDER: &str = "mainWindow/roomPlaceholder";
    /// The name of the open room.
    pub const ROOM_TITLE: &str = "mainWindow/roomTitle";
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

    /// Why the last edit or deletion failed. Visible only then.
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

    // The indicators of a message the server hasn't confirmed, for a test that can't know its id
    // (the client picks it). They exist only while a message is in that state, so the name alone
    // finds it, as long as there is one such message in view.
    pub const UNSENT_PENDING: &str = "mainWindow/msgPending";
    pub const UNSENT_FAILED: &str = "mainWindow/msgFailed";
    pub const UNSENT_RETRY: &str = "mainWindow/msgRetry";
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
    /// The message as written: its `text` is the markdown source.
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
    /// Opens the message's menu (quote, edit, delete). Visible only for a message that can be
    /// quoted or that the user can change; it is clear until the pointer is over the message, but
    /// a click gets to it all the same.
    pub const MENU_BUTTON: &str = "msgMenuButton";
    /// Exists while the message is being sent.
    pub const PENDING: &str = "msgPending";
    /// Exists when the message couldn't be sent.
    pub const FAILED: &str = "msgFailed";
    /// Exists with `FAILED`: sends the message again.
    pub const RETRY: &str = "msgRetry";
}

/// A part of the row of the message with this id (see [`message_row`]). Like rooms, messages are
/// addressed by id: their texts repeat.
#[must_use]
pub fn message(id: &str, part: &str) -> String {
    format!("{}/msg_{id}/{part}", main_window::WINDOW)
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
    fn a_reaction_part_is_addressed_by_the_message_id_and_the_index() {
        assert_eq!(
            reaction("m7", 2, reaction_row::COUNT),
            "mainWindow/msg_m7/reaction2/reactionCount"
        );
        assert_eq!(reaction("m7", 0, ""), "mainWindow/msg_m7/reaction0");
    }
}

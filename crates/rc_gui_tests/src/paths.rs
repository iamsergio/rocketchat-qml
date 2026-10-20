// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Element paths, `<window>/<name>`: the single source of truth for the names each frontend gives
//! its elements (`objectName` in QML, `accessible-id` in Slint). See ARCHITECTURE.md.

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
    /// Sends the text of the composer. Disabled while it is empty.
    pub const COMPOSER_SEND: &str = "mainWindow/composerSend";

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
    pub const TIME: &str = "msgTime";
    pub const BODY: &str = "msgBody";
    /// What a system message says. These have it instead of an author and a body.
    pub const SYSTEM: &str = "msgSystem";
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
}

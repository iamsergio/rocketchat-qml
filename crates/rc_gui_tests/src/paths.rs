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
}

/// The parts of a room row, for [`room`].
pub mod room_row {
    pub const NAME: &str = "roomName";
    /// The marker of the room's kind: `#` channel, a lock for private, `@` direct.
    pub const KIND: &str = "roomKind";
    pub const UNREAD: &str = "roomUnread";
    pub const MENTIONS: &str = "roomMentions";
}

/// A part of the row of the room with this id (see [`room_row`]). Rows are addressed by room id
/// because room names aren't unique.
#[must_use]
pub fn room(id: &str, part: &str) -> String {
    format!("{}/room_{id}/{part}", main_window::WINDOW)
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
}

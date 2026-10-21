// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls.Basic
import RocketChat.Bridge

// The version of the app, for the pages that show before a session: the startup page and the login
// page.
Label {
    objectName: "versionLabel"
    color: Theme.textFaint
    font.pointSize: Theme.fontSizeTiny
    text: qsTr("RocketChatQml %1").arg(AppInfo.version)
}

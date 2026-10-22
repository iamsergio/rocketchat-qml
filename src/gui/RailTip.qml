// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls.Basic

// The tool tip beside a tile of the rail: a name, and a line under it. Not a Popup: a popup takes
// the mouse events of the area it covers, and this one covers the sidebar.
Rectangle {
    id: root

    property string title
    property string subtitle
    property color subtitleColor: Theme.textMuted

    color: Theme.raisedHover
    implicitHeight: lines.implicitHeight + 2 * Theme.spacingM
    implicitWidth: lines.implicitWidth + 2 * Theme.spacingXL
    radius: Theme.radiusControl

    Column {
        id: lines

        anchors.centerIn: parent
        spacing: Theme.spacingNone

        Label {
            color: Theme.text
            font.pointSize: Theme.fontSizeSmall
            font.weight: Theme.fontWeightBold
            text: root.title
            textFormat: Text.PlainText
        }

        Label {
            color: root.subtitleColor
            font.pointSize: Theme.fontSizeSmall
            text: root.subtitle
            textFormat: Text.PlainText
            visible: text !== ""
        }
    }
}

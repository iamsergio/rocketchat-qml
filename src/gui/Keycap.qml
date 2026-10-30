// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls.Basic

// A key in a line of hints: the border of the keycap in the room search field.
Rectangle {
    id: root

    property alias text: capLabel.text

    border.color: Theme.raisedHover
    border.width: Theme.borderWidth
    color: Theme.transparent
    implicitHeight: capLabel.implicitHeight + 2 * Theme.searchHintPaddingV
    implicitWidth: capLabel.implicitWidth + 2 * Theme.searchHintPaddingH
    radius: Theme.radiusCheckBox

    Label {
        id: capLabel

        anchors.centerIn: parent
        color: Theme.textFaint
        font.pointSize: Theme.fontSizeTiny
    }
}

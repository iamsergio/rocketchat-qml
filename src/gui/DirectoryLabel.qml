// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls.Basic

// The label over a group of the search results: "Channels", "People" or "Your rooms".
Item {
    id: root

    // `channels`, `people` or `rooms`, as the model says.
    required property string group

    implicitHeight: Theme.directoryLabelHeight
    width: ListView.view.width

    Label {
        objectName: "directoryGroup_" + root.group
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Theme.directoryLabelPaddingBottom
        anchors.left: parent.left
        anchors.leftMargin: Theme.roomRowInset + Theme.rowPaddingH
        color: Theme.textFaint
        font.capitalization: Font.AllUppercase
        font.letterSpacing: Theme.directoryLabelLetterSpacing
        font.pointSize: Theme.fontSizeTiny
        font.weight: Theme.fontWeightSemibold
        text: root.group === "channels" ? qsTr("Channels") : root.group === "people" ? qsTr("People") : qsTr("Your rooms")
    }
}

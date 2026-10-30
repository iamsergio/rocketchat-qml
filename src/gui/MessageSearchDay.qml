// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls.Basic

// The label over the results of a day in the search pane: "Today", "Yesterday" or the date, in the
// style of the labels over the groups of the room search.
Item {
    id: root

    required property string label

    implicitHeight: Theme.directoryLabelHeight
    width: ListView.view.width

    Label {
        objectName: "messageSearchDay"
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Theme.directoryLabelPaddingBottom
        anchors.left: parent.left
        anchors.leftMargin: Theme.spacingXXXL
        color: Theme.textFaint
        font.capitalization: Font.AllUppercase
        font.letterSpacing: Theme.directoryLabelLetterSpacing
        font.pointSize: Theme.fontSizeTiny
        font.weight: Theme.fontWeightSemibold
        text: root.label
        textFormat: Text.PlainText
    }
}

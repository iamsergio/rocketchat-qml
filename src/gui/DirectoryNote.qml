// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import RocketChat.Bridge

// A row of the search results that is not a room, a person or a channel: that the directory is
// being searched, that it could not be, or that a group has more than it shows.
Item {
    id: root

    required property int count
    required property string group
    required property string rowType
    required property string text

    // Under the names of the rows.
    readonly property int nameInset: Theme.roomRowInset + Theme.roomNameInset

    implicitHeight: rowType === "more" ? Theme.directoryMoreHeight : Theme.roomRowHeight
    width: ListView.view.width

    RowLayout {
        anchors.fill: parent
        anchors.leftMargin: Theme.roomRowInset + Theme.rowPaddingH
        anchors.rightMargin: Theme.roomRowInset + Theme.rowPaddingH
        spacing: Theme.roomRowSpacing
        visible: root.rowType === "searching"

        BusyIndicator {
            Layout.preferredHeight: Theme.directoryBusyIndicatorSize
            Layout.preferredWidth: Theme.directoryBusyIndicatorSize
            implicitHeight: Theme.directoryBusyIndicatorSize
            implicitWidth: Theme.directoryBusyIndicatorSize
            padding: Theme.spacingNone
            palette.dark: Theme.textMuted
            running: visible
        }

        Label {
            objectName: "directorySearching"
            Layout.fillWidth: true
            color: Theme.textFaint
            elide: Text.ElideRight
            font.pointSize: Theme.fontSizeSmall
            text: qsTr("Searching the directory…")
        }
    }

    RowLayout {
        anchors.fill: parent
        anchors.leftMargin: Theme.roomRowInset + Theme.rowPaddingH
        anchors.rightMargin: Theme.roomRowInset + Theme.rowPaddingH
        spacing: Theme.roomRowSpacing
        visible: root.rowType === "failed"

        Label {
            objectName: "directoryError"
            Layout.fillWidth: true
            color: Theme.error
            elide: Text.ElideRight
            font.pointSize: Theme.fontSizeSmall
            text: root.text
            textFormat: Text.PlainText
        }

        Button {
            id: retry

            objectName: "directoryRetry"
            flat: true
            font.pointSize: Theme.fontSizeSmall
            font.weight: Theme.fontWeightSemibold
            padding: Theme.spacingNone
            text: qsTr("Retry")
            onClicked: RoomSearch.retry()

            background: Rectangle {
                color: Theme.transparent
            }

            contentItem: Text {
                color: retry.hovered ? Qt.lighter(Theme.own, Theme.hoverLighter) : Theme.own
                font: retry.font
                text: retry.text
            }
        }
    }

    Label {
        objectName: "directoryMore_" + root.group
        anchors.left: parent.left
        anchors.leftMargin: root.nameInset
        anchors.right: parent.right
        anchors.rightMargin: Theme.roomRowInset + Theme.rowPaddingH
        anchors.verticalCenter: parent.verticalCenter
        color: Theme.textFaint
        elide: Text.ElideRight
        font.pointSize: Theme.fontSizeTiny
        text: qsTr("%1 more, keep typing").arg(root.count)
        visible: root.rowType === "more"
    }
}

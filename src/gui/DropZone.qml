// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts

// Takes the files dropped on the part of a pane it covers, and while some are dragged over it shows
// where they would go. It sits over the messages and the composer, under the pane's header.
Item {
    id: root

    // Where the files would go: "Drop to attach to #general".
    property string title: ""

    // Files were dropped: their `file://` URLs.
    signal dropped(var urls)

    DropArea {
        id: area

        anchors.fill: parent

        onDropped: drop => {
            if (drop.hasUrls)
                root.dropped(drop.urls);
        }
        onEntered: drag => {
            drag.accepted = drag.hasUrls;
        }
    }

    Rectangle {
        objectName: "dropOverlay"
        anchors.fill: parent
        color: Theme.dropOverlay
        visible: area.containsDrag
        z: Theme.zAbove

        Rectangle {
            anchors.fill: parent
            anchors.margins: Theme.dropMargin
            border.color: Theme.ownBorder
            border.width: Theme.dropBorderWidth
            color: Theme.transparent
            radius: Theme.radiusDrop

            ColumnLayout {
                anchors.centerIn: parent
                spacing: Theme.spacingL

                Icon {
                    Layout.alignment: Qt.AlignHCenter
                    Layout.preferredHeight: Theme.dropIconSize
                    Layout.preferredWidth: Theme.dropIconSize
                    source: "icons/upload.svg"
                    tint: Theme.own
                }

                Label {
                    objectName: "dropTitle"
                    Layout.alignment: Qt.AlignHCenter
                    font.pointSize: Theme.fontSizeHeading
                    font.weight: Theme.fontWeightSemibold
                    text: root.title
                    textFormat: Text.PlainText
                }

                Label {
                    Layout.alignment: Qt.AlignHCenter
                    color: Theme.textMuted
                    font.pointSize: Theme.fontSizeSmall
                    text: qsTr("They wait in the composer until you send them")
                }
            }
        }
    }
}

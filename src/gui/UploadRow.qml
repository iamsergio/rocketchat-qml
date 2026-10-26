// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts

// A file being sent: its thumbnail, its name over what it is doing (how much has gone, that it
// waits, that its message is being posted, or why it failed) and a bar of the progress, and what
// can be done with it: cancel it, or for a failed one retry it or remove it. A delegate of the
// composer's UploadListModel; every word comes from Rust.
RowLayout {
    id: root

    required property bool canCancel
    required property bool canRetry
    required property int index
    required property bool isImage
    required property string label
    required property string name
    required property real progress
    required property string status
    required property string statusLine
    required property url thumbnail

    // The buttons were clicked.
    signal cancelRequested
    signal retryRequested

    objectName: "upload" + index
    spacing: Theme.spacingXL

    Thumbnail {
        Layout.alignment: Qt.AlignTop
        isImage: root.isImage
        label: root.label
        source: root.thumbnail
    }

    ColumnLayout {
        Layout.fillWidth: true
        spacing: Theme.spacingXXS

        Label {
            objectName: "uploadName"
            Layout.fillWidth: true
            elide: Text.ElideMiddle
            font.pointSize: Theme.fontSizeSmall
            font.weight: Theme.fontWeightMedium
            text: root.name
            textFormat: Text.PlainText
        }

        Label {
            objectName: "uploadStatus"
            Layout.fillWidth: true
            color: root.status === "failed" ? Theme.error : Theme.textMuted
            font.features: ({
                    "tnum": 1
                })
            font.pointSize: Theme.fontSizeTiny
            text: root.statusLine
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
        }

        Rectangle {
            objectName: "uploadProgress"
            Layout.fillWidth: true
            Layout.topMargin: Theme.spacingS
            color: Theme.raisedHover
            implicitHeight: Theme.uploadTrackHeight
            radius: Theme.radiusTrack
            visible: root.status !== "failed"

            Rectangle {
                color: Theme.own
                height: parent.height
                opacity: root.status === "confirming" ? Theme.uploadConfirmingOpacity : Theme.opaque
                radius: Theme.radiusTrack
                width: parent.width * root.progress
            }
        }
    }

    // What can be done: a failed upload can be tried again, and any but one that posts its message
    // can be given up. The same button for both: a cross, or "Remove" for a failed one.
    RowLayout {
        Layout.alignment: Qt.AlignTop
        spacing: Theme.spacingS

        FlatButton {
            objectName: "uploadRetry" + root.index
            idleColor: Theme.own
            text: qsTr("Retry")
            visible: root.canRetry
            onClicked: root.retryRequested()
        }

        AbstractButton {
            id: cancel

            objectName: "uploadCancel" + root.index
            implicitHeight: root.canRetry ? Theme.buttonHeight : Theme.chipRemoveSize
            implicitWidth: root.canRetry ? removeLabel.implicitWidth + 2 * Theme.buttonPaddingH : Theme.chipRemoveSize
            text: root.canRetry ? qsTr("Remove") : qsTr("Cancel")
            visible: root.canCancel
            onClicked: root.cancelRequested()

            ToolTip.delay: Theme.toolTipDelay
            ToolTip.text: qsTr("Cancel")
            ToolTip.visible: hovered && !root.canRetry

            background: Rectangle {
                color: cancel.hovered ? Theme.raisedHover : Theme.transparent
                radius: Theme.radiusControl
            }

            contentItem: Item {
                Icon {
                    anchors.centerIn: parent
                    height: Theme.chipRemoveIconSize
                    source: "icons/close.svg"
                    tint: cancel.hovered ? Theme.text : Theme.textMuted
                    visible: !root.canRetry
                    width: Theme.chipRemoveIconSize
                }

                Label {
                    id: removeLabel

                    anchors.centerIn: parent
                    color: cancel.hovered ? Theme.text : Theme.textMuted
                    font.pointSize: Theme.fontSizeSmall
                    font.weight: Theme.fontWeightSemibold
                    text: cancel.text
                    visible: root.canRetry
                }
            }
        }
    }
}

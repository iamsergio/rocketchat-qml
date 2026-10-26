// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts

// A file attached to the composer that waits for Send: its thumbnail, its name over its size, and a
// cross that takes it out. A delegate of the composer's StagedFilesModel.
Rectangle {
    id: root

    required property int index
    required property bool isImage
    required property string label
    required property string name
    required property string size
    required property url thumbnail

    // The cross was clicked.
    signal removeRequested

    border.color: Theme.line
    border.width: Theme.borderWidth
    color: Theme.surface
    implicitHeight: Theme.fileChipHeight
    implicitWidth: Theme.fileChipWidth
    objectName: "composerAttachment" + index
    radius: Theme.radiusFileChip

    RowLayout {
        anchors.fill: parent
        anchors.margins: Theme.spacingM
        spacing: Theme.spacingL

        Thumbnail {
            isImage: root.isImage
            label: root.label
            source: root.thumbnail
        }

        ColumnLayout {
            Layout.fillWidth: true
            spacing: Theme.spacingXXS

            Label {
                objectName: "composerAttachmentName"
                Layout.fillWidth: true
                elide: Text.ElideMiddle
                font.pointSize: Theme.fontSizeSmall
                font.weight: Theme.fontWeightMedium
                text: root.name
                textFormat: Text.PlainText
            }

            Label {
                objectName: "composerAttachmentSize"
                color: Theme.textFaint
                font.features: ({
                        "tnum": 1
                    })
                font.pointSize: Theme.fontSizeTiny
                text: root.size
            }
        }

        ToolButton {
            id: remove

            objectName: "composerAttachmentRemove" + root.index
            Layout.alignment: Qt.AlignVCenter
            display: AbstractButton.IconOnly
            implicitHeight: Theme.chipRemoveSize
            implicitWidth: Theme.chipRemoveSize
            padding: Theme.spacingNone
            text: qsTr("Remove %1").arg(root.name)
            onClicked: root.removeRequested()

            ToolTip.delay: Theme.toolTipDelay
            ToolTip.text: qsTr("Remove")
            ToolTip.visible: hovered

            background: Rectangle {
                color: remove.hovered ? Theme.raisedHover : Theme.transparent
                radius: Theme.radiusControl
            }

            contentItem: Item {
                Icon {
                    anchors.centerIn: parent
                    height: Theme.chipRemoveIconSize
                    source: "icons/close.svg"
                    tint: remove.hovered ? Theme.text : Theme.textMuted
                    width: Theme.chipRemoveIconSize
                }
            }
        }
    }
}

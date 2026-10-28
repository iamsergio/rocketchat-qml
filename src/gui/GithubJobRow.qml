// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import RocketChat.Bridge

// One job of the checks: the glyph of its state, its workflow and name, how long it took, and the
// buttons that rerun it (a failed one) and open its log. It lights up while the pointer is over it,
// or over its segment of the bar (`litIndex` is the job whose segment is hovered), and says which
// job the pointer is over with `hovered`. Named `prJob<index>`, the position in the bar.
Item {
    id: root

    required property string duration
    required property int jobIndex
    required property string jobName
    required property string jobState
    required property bool rerunBusy
    required property bool rerunEnabled
    required property bool rerunVisible
    required property string url
    required property string workflow

    property int litIndex: -1

    // The job under the pointer, -1 when it left.
    signal hovered(int index)

    implicitHeight: Math.max(Theme.prJobRowMinHeight, row.implicitHeight + 2 * Theme.spacingS)
    objectName: "prJob" + jobIndex
    width: ListView.view.width

    HoverHandler {
        onHoveredChanged: root.hovered(hovered ? root.jobIndex : -1)
    }

    Rectangle {
        anchors.fill: parent
        anchors.leftMargin: Theme.prChecksPaddingH
        anchors.rightMargin: Theme.prChecksPaddingH
        color: root.litIndex === root.jobIndex ? Theme.raised : Theme.transparent
        radius: Theme.radiusControl
    }

    RowLayout {
        id: row

        anchors.left: parent.left
        anchors.leftMargin: Theme.prChecksPaddingH + Theme.rowPaddingH
        anchors.right: parent.right
        anchors.rightMargin: Theme.prChecksPaddingH + Theme.rowPaddingH
        anchors.verticalCenter: parent.verticalCenter
        spacing: Theme.spacingXL

        // The state: its name is its `text`.
        Item {
            property string text: root.jobState

            Layout.preferredHeight: Theme.prJobGlyphSize
            Layout.preferredWidth: Theme.prJobGlyphSize
            objectName: "prJobState"

            Icon {
                anchors.fill: parent
                source: root.jobState === "cancelled" ? "icons/pr-none.svg" : "icons/pr-" + root.jobState + ".svg"
                tint: Theme.checkColor(root.jobState)
            }
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.spacingNone

            Text {
                objectName: "prJobWorkflow"
                color: Theme.textFaint
                font.pointSize: Theme.fontSizeSmall
                text: root.workflow
                textFormat: Text.PlainText
                visible: text !== ""
            }

            Text {
                objectName: "prJobName"
                Layout.fillWidth: true
                color: Theme.text
                elide: Text.ElideRight
                font.pointSize: Theme.fontSizeSmall
                text: root.jobName
                textFormat: Text.PlainText
            }
        }

        Text {
            objectName: "prJobDuration"
            color: Theme.textFaint
            font.features: ({
                    "tnum": 1
                })
            font.pointSize: Theme.fontSizeTiny
            text: root.duration
            textFormat: Text.PlainText
        }

        GithubButton {
            objectName: "prJobRerun" + root.jobIndex
            busy: root.rerunBusy
            enabled: root.rerunEnabled
            small: true
            text: qsTr("Rerun")
            visible: root.rerunVisible

            onClicked: Github.rerunJob(root.jobIndex)
        }

        ToolButton {
            id: log

            objectName: "prJobLog" + root.jobIndex
            Layout.preferredHeight: Theme.prSmallButtonHeight
            Layout.preferredWidth: Theme.prSmallButtonHeight
            display: AbstractButton.IconOnly
            padding: Theme.spacingNone
            text: qsTr("Open the log")
            visible: root.url !== ""

            ToolTip.delay: Theme.toolTipDelay
            ToolTip.text: text
            ToolTip.visible: hovered

            onClicked: Qt.openUrlExternally(root.url)

            background: Rectangle {
                color: log.hovered ? Theme.raisedHover : Theme.transparent
                radius: Theme.radiusControl
            }

            contentItem: Item {
                Icon {
                    anchors.centerIn: parent
                    height: Theme.prButtonIconSize
                    source: "icons/external.svg"
                    tint: log.hovered ? Theme.text : Theme.textMuted
                    width: Theme.prButtonIconSize
                }
            }
        }
    }
}

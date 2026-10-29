// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import RocketChat.Bridge

// The actions of the pull request that is open: Merge (and Merge anyway when its checks failed) at
// the left, Update branch, Rerun failed jobs and the notify toggle at the right. Merge asks first:
// the question and its answers take the place of the buttons until one is chosen or Escape is
// pressed. Under the bar, why the last command failed, with a button that closes it.
ColumnLayout {
    id: root

    spacing: Theme.spacingNone

    Shortcut {
        enabled: Github.confirmKind !== ""
        sequence: "Escape"

        onActivated: Github.cancelConfirm()
    }

    Item {
        Layout.fillWidth: true
        Layout.minimumHeight: Theme.prActionBarMinHeight
        implicitHeight: Math.max(buttons.implicitHeight, confirm.implicitHeight) + 2 * Theme.spacingXXL

        RowLayout {
            id: buttons

            anchors.left: parent.left
            anchors.leftMargin: Theme.prActionBarPaddingH
            anchors.right: parent.right
            anchors.rightMargin: Theme.prActionBarPaddingH
            anchors.verticalCenter: parent.verticalCenter
            spacing: Theme.spacingXL
            visible: Github.confirmKind === ""

            GithubButton {
                objectName: "prMerge"
                busy: Github.mergeBusy
                enabled: Github.mergeEnabled
                iconSource: "icons/merge.svg"
                look: "primary"
                text: qsTr("Merge")

                onClicked: Github.askMerge(false)
            }

            Text {
                objectName: "prMergeReason"
                color: Theme.textFaint
                font.pointSize: Theme.fontSizeSmall
                text: Github.mergeReason
                verticalAlignment: Text.AlignVCenter
                visible: text !== ""
            }

            GithubButton {
                objectName: "prMergeAnyway"
                busy: Github.mergeAnywayBusy
                enabled: Github.mergeAnywayEnabled
                look: "quiet"
                text: qsTr("Merge anyway")
                visible: Github.mergeAnywayVisible

                onClicked: Github.askMerge(true)
            }

            Item {
                Layout.fillWidth: true
            }

            GithubButton {
                objectName: "prUpdateBranch"
                busy: Github.updateBranchBusy
                enabled: Github.updateBranchEnabled
                text: qsTr("Update branch")
                visible: Github.updateBranchVisible

                onClicked: Github.updateBranch()
            }

            GithubButton {
                objectName: "prRerunFailed"
                busy: Github.rerunFailedBusy
                enabled: Github.rerunFailedEnabled
                text: qsTr("Rerun failed jobs")
                visible: Github.rerunFailedVisible

                onClicked: Github.rerunFailed()
            }

            GithubButton {
                objectName: "prNotify"
                enabled: Github.notifyEnabled
                iconSource: Github.notifying ? "icons/bell-filled.svg" : "icons/bell.svg"
                on: Github.notifying
                text: Github.notifyLabel
                visible: Github.notifyVisible

                onClicked: Github.setNotify(!Github.notifying)
            }
        }

        RowLayout {
            id: confirm

            anchors.left: parent.left
            anchors.leftMargin: Theme.prActionBarPaddingH
            anchors.right: parent.right
            anchors.rightMargin: Theme.prActionBarPaddingH
            anchors.verticalCenter: parent.verticalCenter
            spacing: Theme.spacingXL
            visible: Github.confirmKind !== ""

            Text {
                objectName: "prConfirm"
                Layout.fillWidth: true
                color: Theme.text
                elide: Text.ElideRight
                font.pointSize: Theme.fontSizeSmall
                font.weight: Theme.fontWeightSemibold
                text: Github.confirmText
                verticalAlignment: Text.AlignVCenter
            }

            GithubButton {
                objectName: "prConfirmMerge"
                look: "primary"
                text: qsTr("Merge")
                visible: Github.confirmKind === "merge"

                onClicked: Github.merge(false)
            }

            GithubButton {
                objectName: "prConfirmMergeAnyway"
                look: "signal"
                text: qsTr("Merge anyway")
                visible: Github.confirmKind === "merge-anyway"

                onClicked: Github.merge(true)
            }

            GithubButton {
                objectName: "prConfirmCancel"
                text: qsTr("Cancel")

                onClicked: Github.cancelConfirm()
            }
        }
    }

    Rectangle {
        Layout.fillWidth: true
        color: Theme.line
        implicitHeight: Theme.separatorWidth
    }

    RowLayout {
        Layout.bottomMargin: Theme.prErrorPaddingV
        Layout.fillWidth: true
        Layout.leftMargin: Theme.prActionBarPaddingH
        Layout.rightMargin: Theme.prActionBarPaddingH
        Layout.topMargin: Theme.prErrorPaddingV
        spacing: Theme.spacingXL
        visible: Github.commandError !== ""

        Text {
            objectName: "prError"
            Layout.fillWidth: true
            color: Theme.error
            font.pointSize: Theme.fontSizeSmall
            text: Github.commandError
            verticalAlignment: Text.AlignVCenter
            wrapMode: Text.Wrap
        }

        ToolButton {
            id: dismiss

            objectName: "prErrorDismiss"
            Layout.alignment: Qt.AlignTop
            Layout.preferredHeight: Theme.prSmallButtonHeight
            Layout.preferredWidth: Theme.prSmallButtonHeight
            display: AbstractButton.IconOnly
            padding: Theme.spacingNone
            text: qsTr("Dismiss")

            ToolTip.delay: Theme.toolTipDelay
            ToolTip.text: text
            ToolTip.visible: hovered

            onClicked: Github.dismissCommand()

            background: Rectangle {
                color: dismiss.hovered ? Theme.raised : Theme.transparent
                radius: Theme.radiusControl
            }

            contentItem: Item {
                Icon {
                    anchors.centerIn: parent
                    height: Theme.prButtonIconSize
                    source: "icons/close.svg"
                    tint: dismiss.hovered ? Theme.text : Theme.textMuted
                    width: Theme.prButtonIconSize
                }
            }
        }
    }
}

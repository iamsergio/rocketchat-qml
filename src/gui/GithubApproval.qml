// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Layouts
import RocketChat.Bridge

// The strip over the actions of a pull request whose runs wait for the user's approval, with the
// button that approves them. It shows only then.
Rectangle {
    id: root

    objectName: "prApproval"
    color: Theme.warningWash
    implicitHeight: row.implicitHeight + 2 * Theme.prBannerPaddingV
    visible: Github.approvalText !== ""

    RowLayout {
        id: row

        anchors.fill: parent
        anchors.leftMargin: Theme.prActionBarPaddingH
        anchors.rightMargin: Theme.prActionBarPaddingH
        spacing: Theme.spacingXL

        Icon {
            Layout.preferredHeight: Theme.prBannerIconSize
            Layout.preferredWidth: Theme.prBannerIconSize
            source: "icons/shield.svg"
            tint: Theme.warning
        }

        Text {
            objectName: "prApprovalText"
            Layout.fillWidth: true
            color: Theme.warning
            elide: Text.ElideRight
            font.pointSize: Theme.fontSizeSmall
            text: Github.approvalText
            verticalAlignment: Text.AlignVCenter
        }

        GithubButton {
            objectName: "prApprove"
            busy: Github.approveBusy
            enabled: Github.approveEnabled
            look: "warning"
            small: true
            text: qsTr("Approve runs")

            onClicked: Github.approveRuns()
        }
    }
}

// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import RocketChat.Bridge

// The heading that folds the drafts of the list: "Drafts 4" and a chevron that turns. `checked`
// says the drafts show.
ItemDelegate {
    id: root

    required property bool draftsOpen
    required property string draftsTitle

    checked: draftsOpen
    hoverEnabled: true
    implicitHeight: Theme.roomRowHeight
    leftPadding: Theme.roomRowInset + Theme.rowPaddingH
    objectName: "githubDrafts"
    padding: Theme.spacingNone
    rightPadding: Theme.roomRowInset + Theme.rowPaddingH
    width: ListView.view.width

    onClicked: PullRequestList.toggleDrafts()

    background: Item {
        Rectangle {
            anchors.fill: parent
            anchors.leftMargin: Theme.roomRowInset
            anchors.rightMargin: Theme.roomRowInset
            color: root.hovered ? Theme.surface : Theme.transparent
            radius: Theme.radiusControl
        }
    }

    contentItem: RowLayout {
        spacing: Theme.spacingL

        Icon {
            Layout.preferredHeight: Theme.prFoldChevronSize
            Layout.preferredWidth: Theme.prFoldChevronSize
            rotation: root.draftsOpen ? Theme.prFoldOpenRotation : Theme.prFoldClosedRotation
            source: "icons/chevron.svg"
            tint: Theme.textFaint
        }

        Label {
            objectName: "githubDraftsTitle"
            Layout.fillWidth: true
            color: Theme.textFaint
            font.pointSize: Theme.fontSizeSmall
            font.weight: Theme.fontWeightSemibold
            text: root.draftsTitle
            textFormat: Text.PlainText
        }
    }
}

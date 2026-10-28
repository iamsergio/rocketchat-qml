// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Layouts
import RocketChat.Bridge

// What the last command did ("Merged acme/web #4"), at the bottom of the page for a moment. The
// text stays in the view model until `dismissCommand`, which this calls when the time is up.
Rectangle {
    id: root

    // What a test reads.
    property alias text: label.text

    color: Theme.raisedHover
    implicitHeight: row.implicitHeight + 2 * Theme.prToastPaddingV
    implicitWidth: row.implicitWidth + 2 * Theme.prToastPaddingH
    objectName: "prToast"
    radius: Theme.radiusControl
    visible: Github.toastText !== ""

    Timer {
        id: timer

        interval: Theme.githubToastDuration
        running: root.visible

        onTriggered: Github.dismissCommand()
    }

    Connections {
        function onToastTextChanged() {
            // Another toast right after one gets its whole time.
            if (Github.toastText !== "") {
                timer.restart();
            }
        }

        target: Github
    }

    RowLayout {
        id: row

        anchors.centerIn: parent
        spacing: Theme.spacingL

        Icon {
            Layout.preferredHeight: Theme.prToastIconSize
            Layout.preferredWidth: Theme.prToastIconSize
            source: "icons/check.svg"
            tint: Theme.success
        }

        Text {
            id: label

            color: Theme.text
            font.pointSize: Theme.fontSizeSmall
            text: Github.toastText
            textFormat: Text.PlainText
            verticalAlignment: Text.AlignVCenter
        }
    }
}

// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Layouts
import RocketChat.Bridge

// The GitHub page, in place of an account's: the connect page until GitHub is connected, then the
// list of pull requests in a sidebar and the pull request that is open beside it. The ages and
// "updated N s ago" are shown again every so often while the page is on screen.
Item {
    id: root

    Component.onCompleted: Github.tick()

    Timer {
        interval: Theme.ageInterval
        repeat: true
        running: Github.shown
        onTriggered: Github.tick()
    }

    GithubConnectPage {
        anchors.fill: parent
        visible: Github.connectState !== Github.Connected
    }

    RowLayout {
        anchors.fill: parent
        spacing: Theme.spacingNone
        visible: Github.connectState === Github.Connected

        GithubSidebar {
            Layout.fillHeight: true
            Layout.maximumWidth: Theme.sidebarWidth
            Layout.minimumWidth: Theme.sidebarWidth
            Layout.preferredWidth: Theme.sidebarWidth
        }

        Rectangle {
            Layout.fillHeight: true
            color: Theme.line
            implicitWidth: Theme.separatorWidth
        }

        GithubMain {
            Layout.fillHeight: true
            Layout.fillWidth: true
        }
    }
}

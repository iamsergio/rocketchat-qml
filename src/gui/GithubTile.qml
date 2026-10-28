// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls.Basic
import RocketChat.Bridge

// GitHub in the rail, below the servers: the mark on a tile of the servers' size, greyed out until
// GitHub is connected. A red count says how many of the user's pull requests fail, a short pip
// that one finished while the page wasn't looked at, and the tall pip and the wash that the page is
// on screen. It tells the rail when its tool tip is due (`tipShown`), as a server's tile does. A
// right-click opens the menu with the way to disconnect, when connected.
AbstractButton {
    id: root

    readonly property bool connected: Github.tileConnected
    // What the rail's tool tip reads of its owner, as the tile of a server has them.
    readonly property bool expired: false
    readonly property real freshInset: Theme.spacingNone
    readonly property string tipLine: Github.tipLine
    readonly property string tipTitle: Github.tipTitle
    // The pointer has been over the tile for the tool tip's delay.
    property bool tipShown: false

    Accessible.name: tipTitle
    checked: Github.shown
    height: Theme.railTileSize
    hoverEnabled: true
    objectName: "githubTile"
    padding: Theme.spacingNone
    width: Theme.railWidth

    onClicked: Github.show()
    onHoveredChanged: {
        if (!hovered)
            tipShown = false;
    }

    Timer {
        interval: Theme.toolTipDelay
        running: root.hovered && !menu.visible
        onTriggered: root.tipShown = true
    }

    // A MouseArea, as Spix clicks don't reach a TapHandler. It takes the right button only, so the
    // left one still shows the page.
    MouseArea {
        acceptedButtons: Qt.RightButton
        anchors.fill: parent
        enabled: root.connected
        onClicked: mouse => menu.popup(root, mouse.x, mouse.y)
    }

    // The popup is parented to the window's overlay while open, so Spix finds its item by name from
    // the window (mainWindow/githubDisconnectAction), and only while it is open.
    Menu {
        id: menu

        // Keeps the menu inside the window when it opens near an edge.
        margins: Theme.spacingL

        // The tool tip of the tile would stay over the menu.
        onAboutToShow: root.tipShown = false

        MenuItem {
            objectName: "githubDisconnectAction"
            text: qsTr("Disconnect GitHub")
            onTriggered: Github.disconnect()
        }
    }

    background: Item {
        // A pull request finished: a short pip.
        Rectangle {
            objectName: "githubTilePip"
            anchors.left: parent.left
            anchors.leftMargin: -Theme.railPipWidth
            anchors.verticalCenter: tile.verticalCenter
            color: Theme.text
            height: Theme.railPipUnread
            radius: Theme.railPipWidth
            visible: Github.tilePip && !Github.shown
            // Rounded on the right only: the left half is off the rail.
            width: 2 * Theme.railPipWidth
        }

        // On screen: a tall pip.
        Rectangle {
            objectName: "githubTileCurrent"
            anchors.left: parent.left
            anchors.leftMargin: -Theme.railPipWidth
            anchors.verticalCenter: tile.verticalCenter
            color: Theme.own
            height: Theme.railPipCurrent
            radius: Theme.railPipWidth
            visible: Github.shown
            width: 2 * Theme.railPipWidth
        }

        Rectangle {
            id: tile

            anchors.bottom: parent.bottom
            anchors.horizontalCenter: parent.horizontalCenter
            border.color: {
                if (Github.shown)
                    return Theme.ownBorder;
                return root.connected ? Theme.transparent : Theme.raisedHover;
            }
            border.width: Theme.borderWidth
            color: {
                if (Github.shown)
                    return Theme.ownWash;
                if (!root.connected)
                    return Theme.base;
                return root.hovered ? Theme.raisedHover : Theme.raised;
            }
            height: Theme.railTileSize
            radius: Theme.railTileRadius
            width: Theme.railTileSize

            // How many of the user's pull requests fail, in the signal colour, cut out of the rail's.
            Rectangle {
                color: Theme.rail
                height: Theme.railBadgeHeight + 2 * Theme.railCutoutWidth
                radius: height / 2
                visible: root.connected && Github.tileFailing > 0
                width: Math.max(Theme.railBadgeHeight, failing.implicitWidth + 2 * Theme.railBadgePaddingH) + 2 * Theme.railCutoutWidth
                x: tile.width - width + Theme.railBadgeOverhangX + Theme.railCutoutWidth
                y: tile.height - height + Theme.railBadgeOverhangY + Theme.railCutoutWidth

                Rectangle {
                    anchors.fill: parent
                    anchors.margins: Theme.railCutoutWidth
                    color: Theme.signal
                    radius: height / 2

                    Label {
                        id: failing

                        objectName: "githubTileFailing"
                        anchors.centerIn: parent
                        color: Theme.textOnSignal
                        font.pointSize: Theme.fontSizeTiny
                        font.weight: Theme.fontWeightBold
                        text: Github.tileFailing
                    }
                }
            }
        }
    }

    contentItem: Item {
        Icon {
            objectName: "githubTileMark"
            anchors.centerIn: parent
            height: Theme.githubMarkSize
            source: "icons/mark-github.svg"
            tint: {
                if (Github.shown)
                    return Theme.own;
                if (!root.connected)
                    return Theme.textFaint;
                return root.hovered ? Theme.text : Theme.textMuted;
            }
            width: Theme.githubMarkSize
        }
    }
}

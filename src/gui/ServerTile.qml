// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls.Basic
import RocketChat.Bridge

// One account in the rail: the letter of its server, a pip at the left edge (short for unread, tall
// for the account on screen), the number of mentions, a dot when the login ended and a ring while
// it connects. The tile of an account that isn't added yet (`fresh`) is a dashed plus, with the
// separator above it that otherwise sits before the add button, and is lit only while on screen or
// under the pointer, like that button. Named by
// position, as `serverTile_<index>`: two accounts can have one host. `checked` says it is the one on
// screen. It tells the rail when its tool tip is due (`tipShown`), which the rail draws: a tile in a
// scrolling list can't show a tip outside the list. A right-click opens the menu of the account,
// which is where to log out of it, or to remove it once its login has ended; the tile of an
// account that isn't added yet has none.
AbstractButton {
    id: root

    required property string accountId
    required property bool busy
    required property bool current
    required property bool fresh
    required property int index
    required property string label
    required property int mentions
    required property string monogram
    required property int status
    required property string tipLine
    required property string tipTitle
    required property bool unread

    readonly property bool expired: status === AccountList.Expired
    // What a fresh tile adds above itself for the separator.
    readonly property real freshInset: fresh ? Theme.separatorWidth + Theme.spacingL : Theme.spacingNone
    // The pointer has been over the tile for the tool tip's delay.
    property bool tipShown: false

    Accessible.name: label
    checked: current
    height: Theme.railTileSize + freshInset
    hoverEnabled: true
    objectName: "serverTile_" + index
    padding: Theme.spacingNone
    width: Theme.railWidth

    onClicked: AccountList.select(accountId)
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
    // left one still selects the account.
    MouseArea {
        acceptedButtons: Qt.RightButton
        anchors.fill: parent
        enabled: !root.fresh
        onClicked: mouse => menu.popup(root, mouse.x, mouse.y)
    }

    // The popup is parented to the window's overlay while open, so Spix finds its item by name from
    // the window (mainWindow/logoutAction), and only while it is open.
    Menu {
        id: menu

        // Keeps the menu inside the window when it opens near an edge.
        margins: Theme.spacingL

        // The tool tip of the tile would stay over the menu.
        onAboutToShow: root.tipShown = false

        MenuItem {
            objectName: "logoutAction"
            text: root.expired ? qsTr("Remove this server") : qsTr("Log out")
            onTriggered: AccountList.remove(root.accountId)
        }
    }

    background: Item {
        Rectangle {
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.top: parent.top
            color: Theme.line
            height: Theme.separatorWidth
            visible: root.fresh
            width: Theme.railSeparatorWidth
        }

        // Unread: a short pip.
        Rectangle {
            objectName: "serverTileUnread"
            anchors.left: parent.left
            anchors.leftMargin: -Theme.railPipWidth
            anchors.verticalCenter: tile.verticalCenter
            color: Theme.text
            height: Theme.railPipUnread
            radius: Theme.railPipWidth
            visible: root.unread
            // Rounded on the right only: the left half is off the rail.
            width: 2 * Theme.railPipWidth
        }

        // On screen: a tall pip.
        Rectangle {
            objectName: "serverTileCurrent"
            anchors.left: parent.left
            anchors.leftMargin: -Theme.railPipWidth
            anchors.verticalCenter: tile.verticalCenter
            color: Theme.own
            height: Theme.railPipCurrent
            radius: Theme.railPipWidth
            visible: root.current
            width: 2 * Theme.railPipWidth
        }

        Rectangle {
            id: tile

            anchors.bottom: parent.bottom
            anchors.horizontalCenter: parent.horizontalCenter
            border.color: {
                if (root.fresh)
                    return root.current || root.hovered ? Theme.own : Theme.raisedHover;
                if (root.current)
                    return Theme.ownBorder;
                return root.expired ? Theme.raisedHover : Theme.transparent;
            }
            border.width: root.fresh ? Theme.railNewBorderWidth : Theme.borderWidth
            color: {
                if (root.fresh)
                    return Theme.transparent;
                if (root.current)
                    return Theme.ownWash;
                if (root.expired)
                    return Theme.base;
                return root.hovered ? Theme.raisedHover : Theme.raised;
            }
            height: Theme.railTileSize
            radius: Theme.railTileRadius
            width: Theme.railTileSize

            // Connecting or resuming: an arc turns around the tile.
            Canvas {
                id: ring

                readonly property real arcRadius: Theme.railTileSize / 2 + Theme.railRingGap

                objectName: "serverTileBusy"
                anchors.centerIn: parent
                height: 2 * (arcRadius + Theme.railRingWidth)
                visible: root.busy
                width: height

                onPaint: {
                    const ctx = getContext("2d");
                    ctx.reset();
                    ctx.strokeStyle = Theme.textMuted;
                    ctx.lineWidth = Theme.railRingWidth;
                    ctx.beginPath();
                    ctx.arc(width / 2, height / 2, arcRadius, Theme.railRingArcStart, Theme.railRingArcEnd);
                    ctx.stroke();
                }

                RotationAnimator on rotation {
                    duration: Theme.railRingDuration
                    from: Theme.railRingTurnFrom
                    loops: Animation.Infinite
                    running: ring.visible
                    to: Theme.railRingTurnTo
                }
            }

            // The number of mentions, in the signal colour, cut out of the rail's.
            Rectangle {
                color: Theme.rail
                height: Theme.railBadgeHeight + 2 * Theme.railCutoutWidth
                radius: height / 2
                visible: root.mentions > 0
                width: Math.max(Theme.railBadgeHeight, mentionCount.implicitWidth + 2 * Theme.railBadgePaddingH) + 2 * Theme.railCutoutWidth
                x: tile.width - width + Theme.railBadgeOverhangX + Theme.railCutoutWidth
                y: tile.height - height + Theme.railBadgeOverhangY + Theme.railCutoutWidth

                Rectangle {
                    anchors.fill: parent
                    anchors.margins: Theme.railCutoutWidth
                    color: Theme.signal
                    radius: height / 2

                    Label {
                        id: mentionCount

                        objectName: "serverTileMentions"
                        anchors.centerIn: parent
                        color: Theme.textOnSignal
                        font.pointSize: Theme.fontSizeTiny
                        font.weight: Theme.fontWeightBold
                        text: root.mentions
                        visible: root.mentions > 0
                    }
                }
            }

            // The login ended.
            Rectangle {
                objectName: "serverTileExpired"
                color: Theme.rail
                height: Theme.railDotSize + 2 * Theme.railCutoutWidth
                radius: height / 2
                visible: root.expired
                width: height
                x: tile.width - width + Theme.railDotOverhang + Theme.railCutoutWidth
                y: -Theme.railDotOverhang - Theme.railCutoutWidth

                Rectangle {
                    anchors.fill: parent
                    anchors.margins: Theme.railCutoutWidth
                    color: Theme.warning
                    radius: height / 2
                }
            }
        }
    }

    contentItem: Item {
        Label {
            objectName: "serverTileMonogram"
            anchors.centerIn: parent
            color: {
                if (root.fresh || root.current)
                    return Theme.own;
                if (root.expired)
                    return Theme.textFaint;
                return root.hovered ? Theme.text : Theme.textMuted;
            }
            font.pointSize: Theme.fontSizeRailTile
            font.weight: Theme.fontWeightBold
            text: root.monogram
            textFormat: Text.PlainText
            visible: !root.fresh
        }

        Icon {
            anchors.centerIn: parent
            anchors.verticalCenterOffset: root.freshInset / 2
            height: Theme.railAddIconSize
            source: "icons/plus.svg"
            tint: root.current || root.hovered ? Theme.own : Theme.textFaint
            visible: root.fresh
            width: Theme.railAddIconSize
        }
    }
}

// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import RocketChat.Bridge

// The open room: its name, its messages, oldest at the top, and the box to write in.
Rectangle {
    id: root

    color: Theme.surface

    // Whether the composer takes the focus as soon as it shows, for a room that is being opened.
    property bool focusWhenShown: false

    // Gives the focus to the composer. False, with the focus untouched, if no room is open.
    function focusComposer(): bool {
        if (!composer.visible)
            return false;
        composer.focusField();
        return true;
    }

    // As `focusComposer`, and if no room is open yet, once the one being opened shows.
    function focusComposerOnceOpen() {
        focusWhenShown = !focusComposer();
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: Theme.spacingNone

        // The kind of the open room, its name and under it its topic, over a hairline.
        Item {
            Layout.fillWidth: true
            implicitHeight: Theme.roomHeaderHeight
            visible: Room.roomId !== ""

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: Theme.spacingXXXL
                anchors.rightMargin: Theme.spacingXXXL
                spacing: Theme.spacingXL

                Icon {
                    Layout.preferredHeight: Theme.roomHeaderIconSize
                    Layout.preferredWidth: Theme.roomHeaderIconSize
                    source: Room.kind !== "" ? "icons/" + Room.kind + ".svg" : ""
                    tint: Theme.own
                }

                // Centred, so that the name of a room without a topic is in the middle.
                ColumnLayout {
                    Layout.alignment: Qt.AlignVCenter
                    Layout.fillWidth: true
                    spacing: Theme.spacingXXS

                    Label {
                        objectName: "roomTitle"
                        Layout.fillWidth: true
                        elide: Text.ElideRight
                        font.pointSize: Theme.fontSizeHeading
                        font.weight: Theme.fontWeightBold
                        text: Room.name
                        textFormat: Text.PlainText
                    }

                    // One line, clipped with a fade where it goes on: a Text of rich text can't elide.
                    Item {
                        id: topicLine

                        // The topic is wider than the room has for it.
                        readonly property bool overflowing: topic.implicitWidth > width

                        Layout.fillWidth: true
                        clip: true
                        implicitHeight: topic.implicitHeight
                        visible: Room.topicHtml !== ""

                        Label {
                            id: topic

                            objectName: "roomTopic"
                            color: Theme.textMuted
                            font.pointSize: Theme.fontSizeSmall
                            text: Theme.richTextStyle + Room.topicHtml
                            textFormat: Text.RichText
                            wrapMode: Text.NoWrap
                            onLinkActivated: link => Qt.openUrlExternally(link)

                            LinkArea {
                                label: topic
                                onMenuRequested: (url, origin, x, y) => linkMenu.openFor(url, origin, x, y)
                            }
                        }

                        Rectangle {
                            anchors.bottom: parent.bottom
                            anchors.right: parent.right
                            anchors.top: parent.top
                            visible: topicLine.overflowing
                            width: Theme.roomTopicFadeWidth

                            gradient: Gradient {
                                orientation: Gradient.Horizontal

                                GradientStop {
                                    color: Theme.surfaceClear
                                    position: Theme.gradientStart
                                }

                                GradientStop {
                                    color: Theme.surface
                                    position: Theme.gradientEnd
                                }
                            }
                        }

                        HoverHandler {
                            id: topicHover
                        }

                        // The whole topic, if it doesn't fit.
                        ToolTip {
                            delay: Theme.toolTipDelay
                            visible: topicHover.hovered && topicLine.overflowing
                            width: Math.min(implicitWidth, Theme.roomTopicTipMaxWidth)

                            contentItem: Label {
                                objectName: "roomTopicTip"
                                text: Theme.richTextStyle + Room.topicHtml
                                textFormat: Text.RichText
                                wrapMode: Text.Wrap
                            }
                        }
                    }
                }

                // The search of the room's messages: lit while the pane beside the room shows it.
                ToolButton {
                    id: searchButton

                    objectName: "roomSearchMessagesButton"
                    // Only the tool tip and the accessibility name show it.
                    display: AbstractButton.IconOnly
                    implicitHeight: Theme.iconButtonSize
                    implicitWidth: Theme.iconButtonSize
                    padding: Theme.spacingNone
                    text: qsTr("Search messages (Ctrl+F)")
                    onClicked: ThreadList.toggleSearch()

                    ToolTip.delay: Theme.toolTipDelay
                    ToolTip.text: text
                    ToolTip.visible: hovered

                    background: Rectangle {
                        color: ThreadList.searchOpen ? Theme.ownWash : searchButton.hovered ? Theme.raised : Theme.transparent
                        radius: Theme.radiusControl
                    }

                    contentItem: Item {
                        Icon {
                            anchors.centerIn: parent
                            height: Theme.iconButtonIconSize
                            source: "icons/search.svg"
                            tint: ThreadList.searchOpen ? Theme.own : searchButton.hovered ? Theme.text : Theme.textMuted
                            width: Theme.iconButtonIconSize
                        }
                    }
                }

                // The room's threads: lit while the pane beside the room shows them or a thread.
                ToolButton {
                    id: threadsButton

                    objectName: "roomThreadsButton"
                    // Only the tool tip and the accessibility name show it.
                    display: AbstractButton.IconOnly
                    implicitHeight: Theme.iconButtonSize
                    implicitWidth: Theme.iconButtonSize
                    padding: Theme.spacingNone
                    text: qsTr("Threads")
                    onClicked: ThreadList.toggle()

                    ToolTip.delay: Theme.toolTipDelay
                    ToolTip.text: text
                    ToolTip.visible: hovered

                    background: Rectangle {
                        color: ThreadList.paneOpen ? Theme.ownWash : threadsButton.hovered ? Theme.raised : Theme.transparent
                        radius: Theme.radiusControl
                    }

                    contentItem: Item {
                        Icon {
                            anchors.centerIn: parent
                            height: Theme.iconButtonIconSize
                            source: "icons/thread.svg"
                            tint: ThreadList.paneOpen ? Theme.own : threadsButton.hovered ? Theme.text : Theme.textMuted
                            width: Theme.iconButtonIconSize
                        }
                    }

                    // How many threads have replies the user hasn't read: grey, or red when one of
                    // them mentions the user, with a ring in the header's colour around it.
                    Rectangle {
                        objectName: "roomThreadsBadgeRing"
                        anchors.right: parent.right
                        anchors.rightMargin: -Theme.threadsBadgeOverhangX
                        anchors.top: parent.top
                        anchors.topMargin: -Theme.threadsBadgeOverhangY
                        color: Theme.surface
                        implicitHeight: Theme.threadsBadgeHeight + 2 * Theme.threadsBadgeRing
                        implicitWidth: Math.max(badgeLabel.implicitWidth + 2 * Theme.threadsBadgePaddingH, Theme.threadsBadgeMinWidth) + 2 * Theme.threadsBadgeRing
                        radius: height / 2
                        visible: ThreadList.unreadCount > 0

                        Rectangle {
                            objectName: "roomThreadsBadge"

                            // What the tests read: `mention` while it is red, else `unread`.
                            property string text: ThreadList.unreadMention ? "mention" : "unread"

                            anchors.fill: parent
                            anchors.margins: Theme.threadsBadgeRing
                            color: ThreadList.unreadMention ? Theme.signal : Theme.raisedHover
                            radius: height / 2

                            // Its `text` is the count.
                            Label {
                                id: badgeLabel

                                objectName: "roomThreadsBadgeCount"
                                anchors.centerIn: parent
                                color: ThreadList.unreadMention ? Theme.textOnSignal : Theme.text
                                font.features: ({
                                        "tnum": 1
                                    })
                                font.pointSize: Theme.fontSizeTiny
                                font.weight: Theme.fontWeightBold
                                text: ThreadList.unreadCount
                            }
                        }
                    }
                }
            }

            Rectangle {
                anchors.bottom: parent.bottom
                color: Theme.line
                height: Theme.separatorWidth
                width: parent.width
            }
        }

        Label {
            objectName: "roomPlaceholder"
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: Room.roomId === ""
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            color: Theme.textFaint
            text: qsTr("Select a room")
        }

        Label {
            objectName: "roomError"
            Layout.fillWidth: true
            Layout.margins: Theme.spacingXXL
            visible: text !== ""
            text: Room.errorText
            color: Theme.error
            font.pointSize: Theme.fontSizeSmall
            wrapMode: Text.Wrap
        }

        // The model is newest first and the list runs bottom to top: row 0, the newest message, is at
        // the bottom, where a room opens. Older messages are added after the last row, above everything
        // on screen, so loading them leaves the view where it is. A change of the layout under a view
        // at the newest message does not: Qt keeps the scroll position, so a taller composer or a row
        // that grows after the room opened pushes the newest out of sight. `followsNewest` says that
        // the view was at the newest when it was last in the user's hands, and puts it back there.
        ListView {
            id: messages

            // True when a room opens, and whenever the user stops moving the view at the newest.
            property bool followsNewest: true

            // A window onto the past was put in place and the message to show is yet to be put in
            // the middle of the view: the bottom it shows meanwhile is not one the user reached.
            property bool awaitingShow: false

            // Whether the bottom of the content, the newest message, is the bottom of the view. Worked
            // out from the geometry: `atYEnd` is updated later than the changes that call this.
            function showsNewest() {
                return Math.abs(contentY + height - (originY + contentHeight)) < Theme.scrollTolerance;
            }

            // The view at the newest message again, if it was there and moved only because the layout
            // changed. In this direction the beginning is the newest. Not while the user moves the view,
            // and `showsNewest` keeps this from repositioning when there is nothing to do.
            function followNewest() {
                if (followsNewest && !moving && !scrollBar.pressed && !showsNewest())
                    positionViewAtBeginning();
            }

            // Reaching the bottom of a window onto the past asks for the messages after it; the core
            // ignores the request when a load is in progress or the window is attached.
            function loadNewerAtEnd() {
                if (atYEnd && Room.detached && count > 0 && !awaitingShow)
                    Room.loadNewer();
            }

            // A window shorter than the view has no bottom to scroll to, so it grows by itself.
            onContentHeightChanged: {
                followNewest();
                loadNewerAtEnd();
            }
            onOriginYChanged: followNewest()
            onHeightChanged: followNewest()

            // Wheel, drag and flicks end in a movementEnded. The scroll bar sets the position without
            // moving the view, so it says when it lets go itself.
            onMovementEnded: {
                followsNewest = atYEnd && Room.followsNewest;
                awaitingShow = false;
                loadNewerAtEnd();
            }
            onAtYEndChanged: loadNewerAtEnd()

            objectName: "messageList"
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: Room.roomId !== ""
            clip: true
            focus: true
            model: MessageList
            delegate: MessageDelegate {
                onFocusRequested: messages.forceActiveFocus()
                onEditRequested: messageId => composer.beginEdit(messageId)
                onQuoteRequested: messageId => composer.beginQuote(messageId)
                onReactRequested: messageId => picker.openFor(messageId)
                onLinkMenuRequested: (url, origin, x, y) => linkMenu.openFor(url, origin, x, y)
            }
            verticalLayoutDirection: ListView.BottomToTop
            boundsBehavior: Flickable.StopAtBounds

            ScrollBar.vertical: ScrollBar {
                id: scrollBar

                onPressedChanged: {
                    if (!pressed) {
                        messages.followsNewest = messages.atYEnd && Room.followsNewest;
                        messages.awaitingShow = false;
                        messages.loadNewerAtEnd();
                    }
                }
            }

            // Below the newest message held, which in this direction is the header: a spinner while
            // the messages after a window onto the past load.
            header: Item {
                width: parent?.width ?? 0
                height: newerBusy.visible ? Theme.historyFooterHeight : Theme.spacingNone

                BusyIndicator {
                    id: newerBusy

                    objectName: "loadingNewer"
                    anchors.centerIn: parent
                    palette.dark: Theme.textMuted
                    height: Theme.busyIndicatorSize
                    visible: Room.loadingNewer
                    running: visible
                }
            }

            // Above the oldest message, which in this direction is the footer: the start of the
            // conversation, or a spinner while older messages load.
            footer: Item {
                // The list's content item, as wide as the list. Not ListView.view.width, which qmllint
                // rejects here (attached-property-reuse).
                width: parent?.width ?? 0
                height: startLabel.visible || olderBusy.visible ? Theme.historyFooterHeight : 0

                Label {
                    id: startLabel

                    objectName: "historyStart"
                    anchors.centerIn: parent
                    visible: Room.reachedStart
                    color: Theme.textFaint
                    font.pointSize: Theme.fontSizeSmall
                    text: qsTr("Start of the conversation")
                }

                BusyIndicator {
                    id: olderBusy

                    objectName: "loadingOlder"
                    anchors.centerIn: parent
                    palette.dark: Theme.textMuted
                    height: Theme.busyIndicatorSize
                    visible: Room.loadingOlder
                    running: visible
                }
            }

            // Reaching the top asks for older messages; the core ignores the request when there is
            // nothing to load or a load is in progress. Whatever the layout direction, the beginning
            // of the content is its top in y.
            onAtYBeginningChanged: {
                if (atYBeginning && count > 0)
                    Room.loadOlder();
            }

            Keys.onPressed: event => {
                switch (event.key) {
                case Qt.Key_Home:
                    // First, or the change of the content height that moving causes would put the
                    // view back.
                    followsNewest = false;
                    positionViewAtEnd();
                    break;
                case Qt.Key_End:
                    // A window onto the past does not hold the newest messages: they come with
                    // the reset, which makes the view follow them.
                    if (Room.detached) {
                        Room.showLatest();
                    } else {
                        positionViewAtBeginning();
                        followsNewest = true;
                    }
                    break;
                case Qt.Key_PageUp:
                    followsNewest = false;
                    contentY = Math.max(contentY - height, originY);
                    break;
                case Qt.Key_PageDown:
                    contentY = Math.min(contentY + height, originY + contentHeight - height);
                    followsNewest = showsNewest() && Room.followsNewest;
                    loadNewerAtEnd();
                    break;
                default:
                    return;
                }
                event.accepted = true;
            }

            // A click in the messages takes the keys (End, Home, the page keys) from whatever else
            // had them, such as the field of the search.
            TapHandler {
                onTapped: messages.forceActiveFocus()
            }

            // A room opens at its newest message, and a window onto the past stays where it is.
            // The message a search result stands for is put in the middle of the view.
            Connections {
                function onModelReset() {
                    messages.followsNewest = Room.followsNewest;
                    messages.awaitingShow = Room.detached;
                }

                function onShowRequested(row) {
                    messages.followsNewest = false;
                    messages.awaitingShow = false;
                    messages.positionViewAtIndex(row, ListView.Center);
                }

                target: messages.model
            }

            BusyIndicator {
                objectName: "roomLoading"
                anchors.centerIn: parent
                implicitHeight: Theme.busyIndicatorLargeSize
                implicitWidth: Theme.busyIndicatorLargeSize
                palette.dark: Theme.textMuted
                visible: Room.loading
                running: visible
            }
        }

        // Why the last edit, deletion or reaction failed, until it is dismissed or the user tries
        // again.
        RowLayout {
            objectName: "roomActionError"
            Layout.fillWidth: true
            Layout.margins: Theme.spacingL
            spacing: Theme.spacingL
            visible: Room.actionError !== ""

            Label {
                objectName: "roomActionErrorText"
                Layout.fillWidth: true
                color: Theme.error
                text: Room.actionError
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
            }

            FlatButton {
                objectName: "roomActionErrorDismiss"
                text: qsTr("Dismiss")
                onClicked: Room.dismissActionError()
            }
        }

        // What to do with the link that was right-clicked, in a message or in the topic.
        LinkMenu {
            id: linkMenu
        }

        // The emoji to react with, for the message whose menu or "+" chip asked.
        EmojiPicker {
            id: picker

            property string messageId

            function openFor(id) {
                messageId = id;
                open();
            }

            onPicked: shortcode => Room.react(messageId, shortcode, true)
        }

        // Who is typing in the room. Empty most of the time, but its height is always taken.
        Label {
            objectName: "roomTyping"
            Layout.fillWidth: true
            Layout.leftMargin: Theme.spacingXXXL
            Layout.preferredHeight: Theme.typingLineHeight
            Layout.rightMargin: Theme.spacingXXXL
            color: Theme.textMuted
            elide: Text.ElideRight
            font.pointSize: Theme.fontSizeSmall
            text: Room.typing
            textFormat: Text.PlainText
            verticalAlignment: Text.AlignVCenter
            visible: Room.roomId !== ""
        }

        Composer {
            id: composer

            objectName: "roomComposer"
            Layout.fillWidth: true
            Layout.bottomMargin: Theme.spacingXXXL
            Layout.leftMargin: Theme.spacingXXXL
            Layout.rightMargin: Theme.spacingXXXL
            Layout.topMargin: Theme.spacingM
            visible: Room.roomId !== ""

            onVisibleChanged: {
                if (visible && root.focusWhenShown)
                    focusField();
                root.focusWhenShown = false;
            }
        }
    }

    // Over the bottom of the message list while it is a window onto the past: back to the newest
    // messages.
    Button {
        id: jumpToLatest

        objectName: "roomJumpToLatest"
        anchors.horizontalCenter: parent.horizontalCenter
        height: Theme.jumpToLatestHeight
        horizontalPadding: Theme.jumpToLatestPaddingH
        verticalPadding: Theme.spacingNone
        visible: Room.roomId !== "" && Room.showJumpToLatest
        y: messages.y + messages.height - height - Theme.jumpToLatestMarginBottom
        onClicked: Room.showLatest()

        background: Rectangle {
            border.color: Theme.line
            border.width: Theme.borderWidth
            color: jumpToLatest.hovered ? Theme.raisedHover : Theme.raised
            radius: height / 2
        }

        contentItem: RowLayout {
            spacing: Theme.spacingL

            Icon {
                Layout.preferredHeight: Theme.jumpToLatestIconSize
                Layout.preferredWidth: Theme.jumpToLatestIconSize
                rotation: Theme.jumpToLatestChevronRotation
                source: "icons/chevron.svg"
                tint: Theme.own
            }

            Label {
                color: Theme.text
                font.pointSize: Theme.fontSizeSmall
                text: qsTr("Jump to latest")
                textFormat: Text.PlainText
            }
        }
    }

    // Files dropped on the room go to its composer.
    DropZone {
        anchors.fill: parent
        anchors.topMargin: Theme.roomHeaderHeight
        title: qsTr("Drop to attach to #%1").arg(Room.name)
        visible: Room.roomId !== "" && !Room.preview
        onDropped: urls => composer.attachUrls(urls)
    }
}

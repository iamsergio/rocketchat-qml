// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import RocketChat.Bridge

// The threads of the open room, in the pane beside it: which ones (all, or those the user follows),
// and a row for each, the one with the most recent reply first. ThreadPane shows it while the list
// is what the pane is about, and keeps it, with its place in the list, while a thread is shown.
FocusScope {
    id: root

    // Asks for the next page when the end of the list is in view. The core ignores the request
    // when there is nothing to load or a load is in progress.
    function loadMoreIfAtEnd() {
        if (threads.atYEnd && threads.count > 0)
            ThreadList.loadMore();
    }

    objectName: "threadList"
    // Escape goes back from the thread, or closes the list; the pane decides which.
    Keys.onEscapePressed: ThreadList.escape()

    // One choice of the filter, as wide as its label. It does not take the focus, which stays in the
    // pane for Escape.
    component Segment: AbstractButton {
        id: segment

        required property string filter

        checked: ThreadList.filter === filter
        focusPolicy: Qt.NoFocus
        implicitHeight: Theme.threadFilterHeight
        implicitWidth: segmentLabel.implicitWidth + 2 * Theme.spacingXXL
        onClicked: ThreadList.openList(filter)

        background: Rectangle {
            color: segment.checked ? Theme.raisedHover : segment.hovered ? Theme.raised : Theme.transparent
            radius: Theme.radiusControl
        }

        contentItem: Label {
            id: segmentLabel

            color: segment.checked ? Theme.text : Theme.textMuted
            font.pointSize: Theme.fontSizeSmall
            font.weight: segment.checked ? Theme.fontWeightSemibold : Theme.fontWeightNormal
            horizontalAlignment: Text.AlignHCenter
            text: segment.text
            verticalAlignment: Text.AlignVCenter
        }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: Theme.spacingNone

        // The segmented control: small, at the left, under the header.
        Rectangle {
            Layout.alignment: Qt.AlignLeft
            Layout.bottomMargin: Theme.spacingL
            Layout.leftMargin: Theme.spacingXXXL
            Layout.topMargin: Theme.spacingXL
            color: Theme.base
            implicitHeight: Theme.threadFilterHeight + 2 * Theme.spacingS
            implicitWidth: filters.implicitWidth + 2 * Theme.spacingS
            radius: Theme.radiusControl

            RowLayout {
                id: filters

                anchors.fill: parent
                anchors.margins: Theme.spacingS
                spacing: Theme.spacingS

                Segment {
                    objectName: "threadListFilterAll"
                    filter: "all"
                    text: qsTr("All")
                }

                Segment {
                    objectName: "threadListFilterFollowing"
                    filter: "following"
                    text: qsTr("Following")
                }
            }
        }

        // Why the last request failed, and the way to try again.
        RowLayout {
            Layout.fillWidth: true
            Layout.margins: Theme.spacingXXL
            spacing: Theme.spacingL
            visible: ThreadList.errorText !== ""

            Label {
                objectName: "threadListError"
                Layout.fillWidth: true
                color: Theme.error
                font.pointSize: Theme.fontSizeSmall
                text: ThreadList.errorText
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
            }

            FlatButton {
                objectName: "threadListRetry"
                focusPolicy: Qt.NoFocus
                text: qsTr("Retry")
                onClicked: ThreadList.openList(ThreadList.filter)
            }
        }

        Item {
            Layout.fillHeight: true
            Layout.fillWidth: true

            ListView {
                id: threads

                objectName: "threadListView"
                anchors.fill: parent
                boundsBehavior: Flickable.StopAtBounds
                clip: true
                model: ThreadListModel

                delegate: ThreadListRow {}

                ScrollBar.vertical: ScrollBar {}

                // Under the last row: a spinner while the next page loads.
                footer: Item {
                    // The list's content item, as wide as the list (see RoomView).
                    width: parent?.width ?? 0
                    height: moreBusy.visible ? Theme.historyFooterHeight : Theme.spacingNone

                    BusyIndicator {
                        id: moreBusy

                        objectName: "threadListLoadingMore"
                        anchors.centerIn: parent
                        height: Theme.busyIndicatorSize
                        palette.dark: Theme.textMuted
                        running: visible
                        visible: ThreadList.loadingMore
                    }
                }

                onAtYEndChanged: root.loadMoreIfAtEnd()
                // A page that does not fill the list leaves the end in view: asks for the next.
                onContentHeightChanged: root.loadMoreIfAtEnd()
            }

            BusyIndicator {
                objectName: "threadListLoading"
                anchors.centerIn: parent
                implicitHeight: Theme.busyIndicatorLargeSize
                implicitWidth: Theme.busyIndicatorLargeSize
                palette.dark: Theme.textMuted
                running: visible
                visible: ThreadList.loading
            }

            // What the filter has nothing of.
            ColumnLayout {
                objectName: "threadListEmpty"
                anchors.left: parent.left
                anchors.margins: Theme.spacingXXXL
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                spacing: Theme.spacingL
                visible: ThreadList.empty

                Label {
                    objectName: "threadListEmptyTitle"
                    Layout.fillWidth: true
                    font.weight: Theme.fontWeightSemibold
                    horizontalAlignment: Text.AlignHCenter
                    text: ThreadList.emptyTitle
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                }

                Label {
                    objectName: "threadListEmptyHint"
                    Layout.fillWidth: true
                    color: Theme.textFaint
                    font.pointSize: Theme.fontSizeSmall
                    horizontalAlignment: Text.AlignHCenter
                    text: ThreadList.emptyHint
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                }
            }
        }
    }
}

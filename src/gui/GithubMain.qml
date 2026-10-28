// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import RocketChat.Bridge

// The main area of the GitHub page: the pull request that is open, with its header, the banner for
// runs that wait for approval, the actions and the checks, or a hint to pick one. The toast of a
// command shows at the bottom.
Item {
    id: root

    readonly property bool open: Github.pageId !== ""

    // A branch name.
    component Branch: Rectangle {
        property alias text: name.text

        color: Theme.raised
        implicitHeight: name.implicitHeight + 2 * Theme.spacingXS
        implicitWidth: name.implicitWidth + 2 * Theme.spacingL
        radius: Theme.radiusThumbnail

        Label {
            id: name

            anchors.centerIn: parent
            font.family: Theme.fontFamilyMono
            font.pointSize: Theme.fontSizeTiny
            textFormat: Text.PlainText
        }
    }

    Label {
        objectName: "githubPickHint"
        anchors.centerIn: parent
        color: Theme.textFaint
        text: qsTr("Pick a pull request to see its checks.")
        visible: !root.open
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: Theme.spacingNone
        visible: root.open

        ColumnLayout {
            Layout.bottomMargin: Theme.prHeaderPaddingBottom
            Layout.fillWidth: true
            Layout.leftMargin: Theme.prHeaderPaddingH
            Layout.rightMargin: Theme.prHeaderPaddingH
            Layout.topMargin: Theme.prHeaderPaddingTop
            spacing: Theme.spacingXL

            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.spacingL

                Icon {
                    Layout.preferredHeight: Theme.githubPageMarkSize
                    Layout.preferredWidth: Theme.githubPageMarkSize
                    source: "icons/mark-github.svg"
                    tint: Theme.textMuted
                }

                Label {
                    objectName: "prPageRepo"
                    color: Theme.textMuted
                    text: Github.pageRepo
                    textFormat: Text.PlainText
                }

                Label {
                    objectName: "prPageNumber"
                    color: Theme.textFaint
                    text: Github.pageNumber
                    textFormat: Text.PlainText
                }

                Label {
                    objectName: "prPageUpdated"
                    Layout.fillWidth: true
                    color: Theme.textFaint
                    font.pointSize: Theme.fontSizeSmall
                    text: Github.pageUpdated
                    textFormat: Text.PlainText
                }

                ToolButton {
                    id: openButton

                    objectName: "prPageOpenButton"
                    Layout.preferredHeight: Theme.iconButtonSize
                    Layout.preferredWidth: Theme.iconButtonSize
                    display: AbstractButton.IconOnly
                    padding: Theme.spacingNone
                    text: qsTr("Open on GitHub")

                    ToolTip.delay: Theme.toolTipDelay
                    ToolTip.text: text
                    ToolTip.visible: hovered

                    onClicked: Qt.openUrlExternally(Github.pageUrl)

                    background: Rectangle {
                        color: openButton.hovered ? Theme.raised : Theme.transparent
                        radius: Theme.radiusControl
                    }

                    contentItem: Item {
                        Icon {
                            anchors.centerIn: parent
                            height: Theme.iconButtonIconSize
                            source: "icons/external.svg"
                            tint: openButton.hovered ? Theme.text : Theme.textMuted
                            width: Theme.iconButtonIconSize
                        }
                    }
                }
            }

            Label {
                objectName: "prPageTitle"
                Layout.fillWidth: true
                font.pointSize: Theme.fontSizePageTitle
                font.weight: Theme.fontWeightBold
                text: Github.pageTitle
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
            }

            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.spacingL

                Branch {
                    objectName: "prPageHead"
                    text: Github.pageHead
                }

                Label {
                    color: Theme.textFaint
                    font.pointSize: Theme.fontSizeSmall
                    text: qsTr("into")
                }

                Branch {
                    objectName: "prPageBase"
                    text: Github.pageBase
                }

                Item {
                    Layout.fillWidth: true
                }
            }

            // The review, what the base asks, and whether it is a draft: one line of notes.
            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.spacingXXL
                // Takes no room when there is nothing to note.
                visible: Github.pageReview !== "" || Github.pageBaseNote !== "" || Github.pageDraft

                Label {
                    objectName: "prPageReview"
                    color: Theme.textMuted
                    font.pointSize: Theme.fontSizeSmall
                    text: Github.pageReview
                    textFormat: Text.PlainText
                    visible: text !== ""
                }

                Label {
                    objectName: "prPageBaseNote"
                    color: Theme.warning
                    font.pointSize: Theme.fontSizeSmall
                    text: Github.pageBaseNote
                    textFormat: Text.PlainText
                    visible: text !== ""
                }

                Label {
                    objectName: "prPageDraft"
                    color: Theme.textFaint
                    font.pointSize: Theme.fontSizeSmall
                    text: qsTr("Draft")
                    visible: Github.pageDraft
                }

                Item {
                    Layout.fillWidth: true
                }
            }
        }

        // The line under the header.
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: Theme.separatorWidth
            color: Theme.line
        }

        GithubApproval {
            Layout.fillWidth: true
        }

        GithubActionBar {
            Layout.fillWidth: true
        }

        GithubChecks {
            Layout.fillHeight: true
            Layout.fillWidth: true
        }
    }

    GithubToast {
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Theme.prToastBottomMargin
        anchors.horizontalCenter: parent.horizontalCenter
    }
}

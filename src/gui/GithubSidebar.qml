// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import QtQml.Models
import RocketChat.Bridge

// The sidebar of the GitHub page: the header (who, when it was fetched, and the refresh button),
// the two lists, Mine and Watched, one at a time, the drafts folded under their heading, and the
// self-hosted runners at the foot.
ColumnLayout {
    id: root

    spacing: Theme.spacingNone

    // One of the two lists, as wide as the control. It does not take the focus.
    component Segment: AbstractButton {
        id: segment

        // How many pull requests there are, said after the label in faint text.
        property int count: 0
        required property int tab

        Layout.fillWidth: true
        checked: PullRequestList.tab === tab
        focusPolicy: Qt.NoFocus
        implicitHeight: Theme.threadFilterHeight
        onClicked: PullRequestList.showTab(tab)

        background: Rectangle {
            color: segment.checked ? Theme.raisedHover : segment.hovered ? Theme.raised : Theme.transparent
            radius: Theme.radiusControl
        }

        contentItem: Item {
            RowLayout {
                anchors.centerIn: parent
                spacing: Theme.spacingS

                Label {
                    color: segment.checked ? Theme.text : Theme.textMuted
                    font.pointSize: Theme.fontSizeSmall
                    font.weight: segment.checked ? Theme.fontWeightSemibold : Theme.fontWeightNormal
                    text: segment.text
                }

                Label {
                    objectName: segment.objectName + "Count"
                    color: Theme.textFaint
                    font.features: ({
                            "tnum": 1
                        })
                    font.pointSize: Theme.fontSizeTiny
                    text: segment.count
                }
            }
        }
    }

    RowLayout {
        Layout.bottomMargin: Theme.spacingXL
        Layout.fillWidth: true
        Layout.leftMargin: Theme.sidebarHeaderPaddingH
        Layout.rightMargin: Theme.spacingXL
        Layout.topMargin: Theme.spacingXXL
        spacing: Theme.spacingL

        ColumnLayout {
            Layout.fillWidth: true
            spacing: Theme.spacingXXS

            Label {
                objectName: "githubTitle"
                Layout.fillWidth: true
                elide: Text.ElideRight
                font.pointSize: Theme.fontSizeHeading
                font.weight: Theme.fontWeightBold
                text: Github.headerTitle
                textFormat: Text.PlainText
            }

            // "<login>, updated 40 s ago", "updating…" or why the last fetch failed.
            Label {
                objectName: "githubStatus"
                Layout.fillWidth: true
                color: Theme.textFaint
                elide: Text.ElideRight
                font.pointSize: Theme.fontSizeSmall
                text: Github.headerStatus
                textFormat: Text.PlainText
            }
        }

        ToolButton {
            id: refresh

            objectName: "githubRefreshButton"
            Layout.preferredHeight: Theme.iconButtonSize
            Layout.preferredWidth: Theme.iconButtonSize
            display: AbstractButton.IconOnly
            padding: Theme.spacingNone
            text: qsTr("Refresh")

            ToolTip.delay: Theme.toolTipDelay
            ToolTip.text: text
            ToolTip.visible: hovered

            onClicked: Github.refresh()

            background: Rectangle {
                color: refresh.hovered ? Theme.raised : Theme.transparent
                radius: Theme.radiusControl
            }

            contentItem: Item {
                Icon {
                    anchors.centerIn: parent
                    height: Theme.iconButtonIconSize
                    source: "icons/refresh.svg"
                    tint: refresh.hovered ? Theme.text : Theme.textMuted
                    width: Theme.iconButtonIconSize

                    RotationAnimator on rotation {
                        duration: Theme.spinDuration
                        from: Theme.railRingTurnFrom
                        loops: Animation.Infinite
                        running: Github.refreshing
                        to: Theme.railRingTurnTo
                    }
                }
            }
        }
    }

    // The two lists: small, filling the width, under the header.
    Rectangle {
        Layout.bottomMargin: Theme.spacingL
        Layout.fillWidth: true
        Layout.leftMargin: Theme.spacingXL
        Layout.rightMargin: Theme.spacingXL
        color: Theme.base
        implicitHeight: Theme.threadFilterHeight + 2 * Theme.spacingS
        radius: Theme.radiusControl

        RowLayout {
            anchors.fill: parent
            anchors.margins: Theme.spacingS
            spacing: Theme.spacingS

            Segment {
                objectName: "githubTabMine"
                count: Github.mineCount
                tab: 0
                text: qsTr("Mine")
            }

            Segment {
                objectName: "githubTabWatched"
                count: Github.watchedCount
                tab: 1
                text: qsTr("Watched")
            }
        }
    }

    ListView {
        id: list

        objectName: "githubPrList"
        Layout.fillHeight: true
        Layout.fillWidth: true
        clip: true
        model: PullRequestList

        delegate: DelegateChooser {
            role: "rowType"

            DelegateChoice {
                roleValue: "pull"

                PullRequestRow {
                }
            }

            DelegateChoice {
                roleValue: "drafts"

                DraftsHeading {
                }
            }
        }

        Label {
            objectName: "githubEmpty"
            anchors.centerIn: parent
            color: Theme.textFaint
            horizontalAlignment: Text.AlignHCenter
            text: qsTr("No open pull requests. Ones you open on GitHub show here within a minute.")
            visible: list.count === 0
            width: parent.width - 2 * Theme.sidebarHeaderPaddingH
            wrapMode: Text.Wrap
        }
    }

    // The runners, if the repositories asked for have any.
    Rectangle {
        Layout.fillWidth: true
        color: Theme.line
        implicitHeight: Theme.separatorWidth
        visible: runners.count > 0
    }

    Column {
        Layout.bottomMargin: Theme.spacingXL
        Layout.fillWidth: true
        Layout.leftMargin: Theme.sidebarHeaderPaddingH
        Layout.rightMargin: Theme.sidebarHeaderPaddingH
        Layout.topMargin: Theme.spacingXL
        spacing: Theme.spacingL
        visible: runners.count > 0

        Repeater {
            id: runners

            model: RunnerList

            delegate: RowLayout {
                id: runner

                required property int index
                required property string name
                required property string runnerState
                required property string status

                objectName: "githubRunner" + index
                spacing: Theme.spacingXL
                width: parent.width

                // The lamp, with a halo while the runner is busy or idle.
                Item {
                    Layout.preferredHeight: Theme.runnerLampSize + 2 * Theme.runnerLampHalo
                    Layout.preferredWidth: Theme.runnerLampSize + 2 * Theme.runnerLampHalo

                    Rectangle {
                        anchors.fill: parent
                        color: runner.runnerState === "busy" ? Theme.warningWash : Theme.successWash
                        radius: width / 2
                        visible: runner.runnerState !== "offline"
                    }

                    Rectangle {
                        objectName: "githubRunnerLamp"
                        anchors.centerIn: parent
                        color: {
                            switch (runner.runnerState) {
                            case "busy":
                                return Theme.warning;
                            case "idle":
                                return Theme.success;
                            default:
                                return Theme.textFaint;
                            }
                        }
                        height: Theme.runnerLampSize
                        radius: width / 2
                        width: Theme.runnerLampSize
                    }
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: Theme.spacingNone

                    Label {
                        objectName: "githubRunnerName"
                        Layout.fillWidth: true
                        elide: Text.ElideRight
                        font.pointSize: Theme.fontSizeSmall
                        font.weight: Theme.fontWeightSemibold
                        text: runner.name
                        textFormat: Text.PlainText
                    }

                    Label {
                        objectName: "githubRunnerStatus"
                        Layout.fillWidth: true
                        color: Theme.textFaint
                        elide: Text.ElideRight
                        font.pointSize: Theme.fontSizeTiny
                        text: runner.status
                        textFormat: Text.PlainText
                    }
                }
            }
        }
    }
}

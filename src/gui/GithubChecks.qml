// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import QtQml.Models
import RocketChat.Bridge

// The checks of the pull request that is open: the heading with the tally and the bar of the jobs,
// then the jobs in groups (failed, running, cancelled, passed) that fold. All of it scrolls. The
// pointer over a segment of the bar lights the row of its job and the other way round; a click on
// a segment opens its group if folded and brings its row into view.
ListView {
    id: list

    // The job whose segment or row the pointer is over, -1 for none.
    property int litJob: -1

    // A heading of a group of jobs: `checked` says the group shows its jobs.
    component Group: ItemDelegate {
        id: group

        required property string groupName
        required property bool groupOpen
        required property string groupTitle
        required property string jobState

        checked: groupOpen
        hoverEnabled: true
        implicitHeight: Theme.prGroupHeadingHeight
        leftPadding: Theme.prChecksPaddingH + Theme.rowPaddingH
        objectName: "prJobGroup" + groupName
        padding: Theme.spacingNone
        rightPadding: Theme.prChecksPaddingH + Theme.rowPaddingH
        width: ListView.view.width

        onClicked: JobList.toggleGroup(jobState)

        background: Item {
        }

        contentItem: RowLayout {
            spacing: Theme.spacingL

            Icon {
                Layout.preferredHeight: Theme.prFoldChevronSize
                Layout.preferredWidth: Theme.prFoldChevronSize
                rotation: group.groupOpen ? Theme.prFoldOpenRotation : Theme.prFoldClosedRotation
                source: "icons/chevron.svg"
                tint: group.hovered ? Theme.text : Theme.textFaint
            }

            Text {
                objectName: "prJobGroupTitle"
                Layout.fillWidth: true
                color: group.hovered ? Theme.text : Theme.textFaint
                font.pointSize: Theme.fontSizeSmall
                font.weight: Theme.fontWeightSemibold
                text: group.groupTitle
                textFormat: Text.PlainText
                verticalAlignment: Text.AlignVCenter
            }
        }
    }

    bottomMargin: Theme.prChecksPaddingV
    clip: true
    model: JobList
    objectName: "prChecks"
    topMargin: Theme.prChecksPaddingV

    delegate: DelegateChooser {
        role: "rowType"

        DelegateChoice {
            roleValue: "group"

            Group {
            }
        }

        DelegateChoice {
            roleValue: "job"

            GithubJobRow {
                litIndex: list.litJob

                onHovered: index => list.litJob = index
            }
        }
    }

    header: Item {
        implicitHeight: overview.implicitHeight + Theme.prChecksSpacing
        width: list.width

        ColumnLayout {
            id: overview

            anchors.left: parent.left
            anchors.leftMargin: Theme.prChecksPaddingH
            anchors.right: parent.right
            anchors.rightMargin: Theme.prChecksPaddingH
            spacing: Theme.prChecksSpacing

            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.prTallySpacing

                Text {
                    objectName: "prChecksHeading"
                    color: Theme.text
                    font.pointSize: Theme.fontSizeHeading
                    font.weight: Theme.fontWeightBold
                    text: Github.checksHeading
                    textFormat: Text.PlainText
                }

                // The tally, a swatch of the state's colour before each count. Its `text` is the
                // whole of it.
                Row {
                    objectName: "prTally"
                    property string text: Github.checksTally

                    spacing: Theme.prTallySpacing

                    Repeater {
                        model: Github.tallyTexts

                        delegate: Row {
                            id: part

                            required property int index
                            required property string modelData

                            spacing: Theme.spacingM

                            Rectangle {
                                anchors.verticalCenter: parent.verticalCenter
                                color: Theme.checkColor(Github.tallyStates[part.index])
                                height: Theme.prTallySwatchSize
                                radius: Theme.radiusTrack
                                width: Theme.prTallySwatchSize
                            }

                            Text {
                                color: Theme.textMuted
                                font.features: ({
                                        "tnum": 1
                                    })
                                font.pointSize: Theme.fontSizeSmall
                                text: part.modelData
                                textFormat: Text.PlainText
                            }
                        }
                    }
                }

                Item {
                    Layout.fillWidth: true
                }
            }

            CheckBar {
                objectName: "prChecksBar"
                Layout.fillWidth: true
                dimPassed: false
                interactive: true
                litIndex: list.litJob
                segments: Github.checksBar
                visible: segments.length > 0

                onSegmentClicked: index => {
                    const row = JobList.reveal(index);
                    if (row >= 0) {
                        Qt.callLater(() => list.positionViewAtIndex(row, ListView.Contain));
                    }
                }
                onSegmentHovered: index => list.litJob = index
            }
        }
    }
}

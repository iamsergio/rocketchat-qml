// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import RocketChat.Bridge

// One pull request in the list: its state, the repository (and the author, on the Watched tab) over
// its title and the bar of its jobs. Named by position, `prRow<index>`, as the list is ordered by
// what changed last: a test finds a row by its title. `checked` says the page shows it.
ItemDelegate {
    id: root

    required property string age
    required property bool bell
    required property var bar
    required property bool changed
    required property string glyph
    required property int index
    required property string number
    required property string prId
    required property string repoLine
    required property string titleStyled

    readonly property bool selected: prId === Github.pageId

    checked: selected
    hoverEnabled: true
    implicitHeight: row.implicitHeight + 2 * Theme.prRowPaddingV
    leftPadding: Theme.roomRowInset + Theme.rowPaddingH
    objectName: "prRow" + index
    padding: Theme.spacingNone
    rightPadding: Theme.roomRowInset + Theme.rowPaddingH
    width: ListView.view.width

    onClicked: PullRequestList.select(index)

    background: Item {
        Rectangle {
            anchors.fill: parent
            anchors.leftMargin: Theme.roomRowInset
            anchors.rightMargin: Theme.roomRowInset
            color: root.selected ? Theme.ownWash : root.hovered ? Theme.surface : Theme.transparent
            radius: Theme.radiusControl
        }
    }

    contentItem: RowLayout {
        id: row

        spacing: Theme.spacingXL

        // The state: its name is its `text`.
        Item {
            property string text: root.glyph

            Layout.alignment: Qt.AlignTop
            Layout.preferredHeight: Theme.prGlyphSize
            Layout.preferredWidth: Theme.prGlyphSize
            Layout.topMargin: Theme.spacingXS
            objectName: "prGlyph"

            Icon {
                anchors.fill: parent
                source: "icons/pr-" + root.glyph + ".svg"
                tint: {
                    switch (root.glyph) {
                    case "failed":
                        return Theme.signal;
                    case "running":
                        return Theme.warning;
                    case "passed":
                        return Theme.success;
                    case "waiting":
                        return Theme.own;
                    default:
                        return Theme.textFaint;
                    }
                }
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            spacing: Theme.spacingNone

            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.spacingS

                Label {
                    objectName: "prRepo"
                    Layout.fillWidth: true
                    color: Theme.textFaint
                    elide: Text.ElideRight
                    font.pointSize: Theme.fontSizeTiny
                    text: root.repoLine
                    textFormat: Text.PlainText
                }

                Icon {
                    objectName: "prBell"
                    Layout.preferredHeight: Theme.prBellSize
                    Layout.preferredWidth: Theme.prBellSize
                    source: "icons/bell.svg"
                    tint: Theme.own
                    visible: root.bell
                }

                Label {
                    objectName: "prAge"
                    color: Theme.textFaint
                    font.pointSize: Theme.fontSizeTiny
                    text: root.age
                    textFormat: Text.PlainText
                }
            }

            // The title, then the number in faint text; two lines at most.
            Label {
                objectName: "prTitle"
                Layout.fillWidth: true
                color: Theme.text
                elide: Text.ElideRight
                font.pointSize: Theme.fontSizeSmall
                font.weight: root.changed ? Theme.fontWeightBold : Theme.fontWeightNormal
                maximumLineCount: 2
                text: root.titleStyled + " " + Theme.richFaintStart + root.number + Theme.richFaintEnd
                textFormat: Text.StyledText
                wrapMode: Text.Wrap
            }

            CheckBar {
                objectName: "prBar"
                Layout.fillWidth: true
                Layout.topMargin: Theme.prCheckBarGap
                barHeight: Theme.checkBarHeightRow
                dimPassed: !root.selected
                segments: root.bar
                visible: root.bar.length > 0
            }
        }
    }
}

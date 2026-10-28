// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

pragma ComponentBehavior: Bound

import QtQuick

// The checks of a pull request as a bar: one segment per job, of equal width, in the colour of how
// the job stands. `segments` names them (failed, running, cancelled, passed or waiting). The
// segments of passed jobs are dimmed, unless `dimPassed` is off. Segment `i` is `prSegment<i>`,
// whose `text` is its name. An `interactive` bar tells which segment the pointer is over
// (`segmentHovered`, -1 when none) and which one is clicked (`segmentClicked`), and grows the
// segment `litIndex` taller, which the owner sets to the one that is hovered, here or in a list
// of the jobs.
Row {
    id: root

    property bool dimPassed: true
    property bool interactive: false
    property int litIndex: -1
    property int barHeight: Theme.checkBarHeight
    property var segments: []

    height: barHeight
    spacing: Theme.checkBarGap

    signal segmentClicked(int index)
    signal segmentHovered(int index)

    Repeater {
        id: repeater

        model: root.segments

        delegate: Rectangle {
            id: segment

            required property int index
            required property string modelData

            // What a test reads, as it can't see the colour.
            property string text: modelData
            readonly property bool lit: root.interactive && root.litIndex === index

            color: Theme.checkColor(modelData)
            height: root.barHeight
            objectName: "prSegment" + index
            opacity: modelData === "passed" && root.dimPassed ? Theme.checkPassedOpacity : Theme.opaque
            radius: Theme.radiusTrack
            width: (root.width - (repeater.count - 1) * Theme.checkBarGap) / repeater.count

            transform: Scale {
                origin.y: segment.height / 2
                yScale: segment.lit ? Theme.checkBarHoverScale : Theme.checkBarRestScale

                Behavior on yScale {
                    NumberAnimation {
                        duration: Theme.checkBarHoverDuration
                    }
                }
            }

            // A MouseArea, as Spix clicks don't reach a TapHandler.
            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                enabled: root.interactive
                hoverEnabled: true

                onClicked: root.segmentClicked(segment.index)
                onEntered: root.segmentHovered(segment.index)
                onExited: root.segmentHovered(-1)
            }
        }
    }
}

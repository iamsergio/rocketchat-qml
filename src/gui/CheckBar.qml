// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

pragma ComponentBehavior: Bound

import QtQuick

// The checks of a pull request as a bar: one segment per job, of equal width, in the colour of how
// the job stands. `segments` names them (failed, running, cancelled, passed or waiting). The
// segments of passed jobs are dimmed, unless `dimPassed` is off. Segment `i` is `prSegment<i>`,
// whose `text` is its name.
Row {
    id: root

    property bool dimPassed: true
    property int barHeight: Theme.checkBarHeight
    property var segments: []

    height: barHeight
    spacing: Theme.checkBarGap

    Repeater {
        id: repeater

        model: root.segments

        delegate: Rectangle {
            id: segment

            required property int index
            required property string modelData

            // What a test reads, as it can't see the colour.
            property string text: modelData

            color: {
                switch (modelData) {
                case "failed":
                    return Theme.signal;
                case "running":
                    return Theme.warning;
                case "passed":
                    return Theme.success;
                case "cancelled":
                    return Theme.textFaint;
                default:
                    return Theme.raisedHover;
                }
            }
            height: root.barHeight
            objectName: "prSegment" + index
            opacity: modelData === "passed" && root.dimPassed ? Theme.checkPassedOpacity : Theme.opaque
            radius: Theme.radiusTrack
            width: (root.width - (repeater.count - 1) * Theme.checkBarGap) / repeater.count
        }
    }
}

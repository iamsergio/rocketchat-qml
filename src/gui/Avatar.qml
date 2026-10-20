// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Effects

// A rounded avatar: the image once it has loaded, the initials on a colour until then. The image and
// the placeholder are named by the user of the component, as "msgAvatar" and "roomAvatar".
Item {
    id: root

    // A file URL, or empty while there is no avatar file.
    required property string url
    required property string initials
    required property color fillColor
    required property string imageName
    required property string placeholderName
    required property string initialsName
    readonly property bool loaded: image.status === Image.Ready

    implicitWidth: 32
    implicitHeight: 32

    Rectangle {
        objectName: root.placeholderName
        anchors.fill: parent
        radius: width / 4
        color: root.fillColor
        visible: !root.loaded

        Label {
            objectName: root.initialsName
            anchors.centerIn: parent
            color: "white"
            font.bold: true
            font.pixelSize: root.height * 0.4
            text: root.initials
            textFormat: Text.PlainText
        }
    }

    Image {
        id: image

        // The URL, which is what the tests read: Spix reads text.
        property string text: root.url

        objectName: root.imageName
        anchors.fill: parent
        asynchronous: true
        fillMode: Image.PreserveAspectCrop
        source: root.url
        visible: status === Image.Ready
        // The software scene graph (under the offscreen platform, for one) draws no layer
        // effects: there the corners stay square.
        layer.enabled: GraphicsInfo.api !== GraphicsInfo.Software
        layer.effect: MultiEffect {
            maskEnabled: true
            maskSource: mask
        }
    }

    Item {
        id: mask

        anchors.fill: parent
        layer.enabled: true
        visible: false

        Rectangle {
            anchors.fill: parent
            radius: width / 4
        }
    }
}

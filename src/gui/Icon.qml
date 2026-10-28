// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Effects

// A monochrome SVG icon in the colour `tint`. The SVGs are white: colorizing keeps the luminance,
// so a white source takes the colour as it is.
//
// An Item around the image rather than an Image: the image is rasterised at the size the icon is
// given, and an Image takes its implicit size from that, which is a binding loop when a layout
// leaves an icon (a hidden one) at its implicit size. The Item's implicit size is nothing: the size
// is always given, by width and height, anchors or the layout.
Item {
    id: root

    property color tint: Theme.text
    property alias source: image.source
    readonly property alias status: image.status

    Image {
        id: image

        anchors.fill: parent
        fillMode: Image.PreserveAspectFit
        // Rasterised at the size shown, in device pixels.
        sourceSize.width: width * Screen.devicePixelRatio
        sourceSize.height: height * Screen.devicePixelRatio
        // The software scene graph (under the offscreen platform, for one) draws no layer effects:
        // there the icon stays white.
        layer.enabled: GraphicsInfo.api !== GraphicsInfo.Software
        layer.effect: MultiEffect {
            colorization: 1
            colorizationColor: root.tint
        }
    }
}

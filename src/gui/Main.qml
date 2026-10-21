// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls.Basic
import RocketChat.Bridge

ApplicationWindow {
    id: root

    objectName: "mainWindow"
    width: Theme.windowWidth
    height: Theme.windowHeight
    visible: true
    title: qsTr("RocketChatQml")
    color: Theme.base
    // Every control and label that doesn't set a size of its own takes it from here.
    font.pointSize: Theme.fontSizeBase

    // What the controls we don't restyle read: the menus, the dialogs, the scroll bars and the tool
    // tips. `window` is the colour of popups, not of the window, which `color` above sets: the
    // popups sit on top of it, so they are raised.
    palette {
        button: Theme.raised
        buttonText: Theme.text
        dark: Theme.raisedHover
        highlight: Theme.own
        highlightedText: Theme.textOnOwn
        light: Theme.raisedHover
        link: Theme.own
        linkVisited: Theme.own
        mid: Theme.raisedHover
        midlight: Theme.raisedHover
        placeholderText: Theme.textFaint
        shadow: Theme.shadow
        text: Theme.text
        toolTipBase: Theme.raisedHover
        toolTipText: Theme.text
        window: Theme.raised
        windowText: Theme.text
        base: Theme.surface
        alternateBase: Theme.surface
        brightText: Theme.text
    }

    // One page per session state. A Loader, so that a page starts empty each time it shows: the
    // login fields don't keep the password after a logout. While logging in the state is
    // Connecting, which keeps the login page and its fields. Resuming a stored session at startup
    // shows the startup page, so that the login page doesn't flash before the room list.
    Loader {
        anchors.fill: parent
        source: {
            switch (Session.state) {
            case Session.LoggedIn:
                return "RoomPage.qml";
            case Session.Unknown:
            case Session.Resuming:
                return "StartupPage.qml";
            default:
                return "LoginPage.qml";
            }
        }
    }
}

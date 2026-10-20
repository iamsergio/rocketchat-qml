// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import RocketChat.Bridge

Item {
    ColumnLayout {
        anchors.centerIn: parent
        width: Math.min(parent.width - 32, 360)
        spacing: 12

        Label {
            Layout.alignment: Qt.AlignHCenter
            text: qsTr("Log in to Rocket.Chat")
            font.pixelSize: 20
        }

        TextField {
            id: server

            objectName: "loginServerField"
            Layout.fillWidth: true
            enabled: !Session.busy
            placeholderText: qsTr("Server, e.g. https://chat.example.com")
            inputMethodHints: Qt.ImhUrlCharactersOnly | Qt.ImhNoAutoUppercase
        }

        TextField {
            id: username

            objectName: "loginUsernameField"
            Layout.fillWidth: true
            enabled: !Session.busy
            placeholderText: qsTr("Username")
            inputMethodHints: Qt.ImhNoAutoUppercase
        }

        TextField {
            id: password

            objectName: "loginPasswordField"
            Layout.fillWidth: true
            enabled: !Session.busy
            placeholderText: qsTr("Password")
            echoMode: TextInput.Password
            onAccepted: loginButton.click()
        }

        Label {
            objectName: "loginError"
            Layout.fillWidth: true
            visible: text !== ""
            text: Session.errorText
            color: palette.link
            wrapMode: Text.Wrap
        }

        RowLayout {
            Layout.fillWidth: true

            Button {
                id: loginButton

                objectName: "loginButton"
                text: qsTr("Log in")
                enabled: !Session.busy
                onClicked: Session.login(server.text, username.text, password.text)
            }

            BusyIndicator {
                objectName: "loginBusy"
                visible: Session.busy
                running: visible
            }
        }
    }
}

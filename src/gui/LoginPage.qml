// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import RocketChat.Bridge

// The fields start empty, or filled in for the local test server when a developer build got --dev.
Item {
    ColumnLayout {
        anchors.centerIn: parent
        width: Math.min(parent.width - Theme.pageMargin, Theme.loginFormWidth)
        spacing: Theme.spacingXXL

        Label {
            Layout.alignment: Qt.AlignHCenter
            text: qsTr("Log in to Rocket.Chat")
            font.pointSize: Theme.fontSizeTitle
            font.weight: Theme.fontWeightBold
        }

        // A label above each field. Its name says what the field takes, where the placeholder gives
        // an example.
        ColumnLayout {
            Layout.fillWidth: true
            spacing: Theme.spacingM

            FieldLabel {
                text: qsTr("Server")
            }

            InputField {
                id: server

                objectName: "loginServerField"
                Layout.fillWidth: true
                text: Session.prefillServer
                enabled: !Session.busy
                placeholderText: qsTr("Server, e.g. https://chat.example.com")
                inputMethodHints: Qt.ImhUrlCharactersOnly | Qt.ImhNoAutoUppercase
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            spacing: Theme.spacingM

            FieldLabel {
                text: qsTr("Username")
            }

            InputField {
                id: username

                objectName: "loginUsernameField"
                Layout.fillWidth: true
                text: Session.prefillUsername
                enabled: !Session.busy
                placeholderText: qsTr("Username")
                inputMethodHints: Qt.ImhNoAutoUppercase
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            spacing: Theme.spacingM

            FieldLabel {
                text: qsTr("Password")
            }

            InputField {
                id: password

                objectName: "loginPasswordField"
                Layout.fillWidth: true
                text: Session.prefillPassword
                enabled: !Session.busy
                placeholderText: qsTr("Password")
                echoMode: TextInput.Password
                onAccepted: loginButton.click()
            }
        }

        Label {
            objectName: "loginError"
            Layout.fillWidth: true
            visible: text !== ""
            text: Session.errorText
            color: Theme.error
            font.pointSize: Theme.fontSizeSmall
            wrapMode: Text.Wrap
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.spacingL

            PrimaryButton {
                id: loginButton

                objectName: "loginButton"
                Layout.fillWidth: true
                font.pointSize: Theme.fontSizeBase
                implicitHeight: Theme.fieldHeight
                text: qsTr("Log in")
                enabled: !Session.busy
                onClicked: Session.login(server.text, username.text, password.text)
            }

            BusyIndicator {
                objectName: "loginBusy"
                Layout.preferredHeight: Theme.busyIndicatorSize
                Layout.preferredWidth: Theme.busyIndicatorSize
                palette.dark: Theme.textMuted
                visible: Session.busy
                running: visible
            }
        }
    }

    VersionLabel {
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Theme.spacingXXL
        anchors.horizontalCenter: parent.horizontalCenter
    }
}

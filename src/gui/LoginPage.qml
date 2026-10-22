// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import RocketChat.Bridge

// The login page of the account on screen, in one of three modes (`Session.loginMode`, with the
// texts). Login: the fields start with who the account is, if it says so, else empty, or filled in
// for the local test server when a developer build got --dev. Adding: the account is a new one,
// and there are others to go back to with Cancel. Again: the session of the account ended, so the
// server can't be changed and the account can be removed. Main.qml makes the page anew whenever
// another account is on screen.
Item {
    id: root

    readonly property bool again: Session.loginMode === Session.Again
    readonly property bool adding: Session.loginMode === Session.Adding

    Component.onCompleted: {
        if (again)
            password.forceActiveFocus();
    }

    ColumnLayout {
        anchors.centerIn: parent
        width: Math.min(parent.width - Theme.pageMargin, Theme.loginFormWidth)
        spacing: Theme.spacingXXL

        Label {
            objectName: "loginTitle"
            Layout.fillWidth: true
            text: Session.loginTitle
            font.pointSize: Theme.fontSizeTitle
            font.weight: Theme.fontWeightBold
            wrapMode: Text.Wrap
        }

        Label {
            objectName: "loginLede"
            Layout.fillWidth: true
            color: Theme.textMuted
            text: Session.loginLede
            visible: text !== ""
            wrapMode: Text.Wrap
        }

        // The session ended: say where.
        Rectangle {
            Layout.fillWidth: true
            color: Theme.warningWash
            implicitHeight: notice.implicitHeight + 2 * Theme.spacingXL
            radius: Theme.radiusControl
            visible: Session.loginNotice !== ""

            Icon {
                id: noticeIcon

                anchors.left: parent.left
                anchors.leftMargin: Theme.spacingXXL
                anchors.top: parent.top
                anchors.topMargin: Theme.spacingXL + Theme.spacingXXS
                height: Theme.roomIconSize
                source: "icons/clock.svg"
                tint: Theme.warning
                width: Theme.roomIconSize
            }

            Label {
                id: notice

                objectName: "loginNotice"
                anchors.left: noticeIcon.right
                anchors.leftMargin: Theme.spacingXL
                anchors.right: parent.right
                anchors.rightMargin: Theme.spacingXXL
                anchors.top: parent.top
                anchors.topMargin: Theme.spacingXL
                color: Theme.warning
                font.pointSize: Theme.fontSizeSmall
                text: Session.loginNotice
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
            }
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
                text: Session.loginServer
                enabled: !Session.busy
                readOnly: root.again
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
                text: Session.loginUsername
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

            FlatButton {
                objectName: "loginCancelButton"
                font.pointSize: Theme.fontSizeBase
                implicitHeight: Theme.fieldHeight
                text: qsTr("Cancel")
                visible: root.adding
                // Removes the new account, which shows another one again.
                onClicked: Session.logout()
            }

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

        FlatButton {
            objectName: "loginRemoveServerButton"
            Layout.alignment: Qt.AlignHCenter
            enabled: !Session.busy
            text: qsTr("Remove this server")
            visible: root.again
            onClicked: Session.logout()
        }
    }

    VersionLabel {
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Theme.spacingXXL
        anchors.horizontalCenter: parent.horizontalCenter
    }
}

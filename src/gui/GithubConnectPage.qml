// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import RocketChat.Bridge

// GitHub isn't connected: the main area, with no sidebar, like the login page. Two ways: the login
// of the GitHub CLI, or a personal access token. Why a try failed is said under the buttons.
Item {
    id: root

    readonly property bool idle: Github.connectState !== Github.Connecting

    ColumnLayout {
        anchors.centerIn: parent
        spacing: Theme.spacingXXL
        width: Math.min(parent.width - Theme.pageMargin, Theme.githubConnectWidth)

        Icon {
            Layout.preferredHeight: Theme.githubConnectMarkSize
            Layout.preferredWidth: Theme.githubConnectMarkSize
            source: "icons/mark-github.svg"
            tint: Theme.text
        }

        Label {
            objectName: "githubConnectTitle"
            Layout.fillWidth: true
            font.pointSize: Theme.fontSizeTitle
            font.weight: Theme.fontWeightBold
            text: qsTr("Connect GitHub")
            wrapMode: Text.Wrap
        }

        Label {
            Layout.fillWidth: true
            color: Theme.textMuted
            text: qsTr("See your open pull requests and their CI next to your chats. Nothing is sent to your Rocket.Chat servers.")
            wrapMode: Text.Wrap
        }

        // The login of the gh command.
        Rectangle {
            Layout.fillWidth: true
            border.color: Theme.raisedHover
            border.width: Theme.borderWidth
            color: Theme.surface
            implicitHeight: option.implicitHeight + 2 * Theme.spacingXXL
            radius: Theme.radiusControl

            RowLayout {
                id: option

                anchors.fill: parent
                anchors.margins: Theme.spacingXXL
                spacing: Theme.spacingXXL

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: Theme.spacingXXS

                    Label {
                        Layout.fillWidth: true
                        font.weight: Theme.fontWeightSemibold
                        text: qsTr("Use the GitHub CLI login")
                        wrapMode: Text.Wrap
                    }

                    Label {
                        Layout.fillWidth: true
                        color: Theme.textFaint
                        font.pointSize: Theme.fontSizeSmall
                        text: qsTr("Uses the token of the gh command")
                        wrapMode: Text.Wrap
                    }
                }

                PrimaryButton {
                    objectName: "githubConnectGh"
                    enabled: root.idle
                    text: qsTr("Use it")
                    onClicked: Github.connectWithGh()
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.spacingXL

            Rectangle {
                Layout.fillWidth: true
                color: Theme.line
                implicitHeight: Theme.separatorWidth
            }

            Label {
                color: Theme.textFaint
                font.pointSize: Theme.fontSizeSmall
                text: qsTr("or")
            }

            Rectangle {
                Layout.fillWidth: true
                color: Theme.line
                implicitHeight: Theme.separatorWidth
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            spacing: Theme.spacingM

            FieldLabel {
                text: qsTr("Personal access token")
            }

            InputField {
                id: token

                objectName: "githubConnectTokenField"
                Layout.fillWidth: true
                echoMode: TextInput.Password
                enabled: root.idle
                font.family: Theme.fontFamilyMono
                inputMethodHints: Qt.ImhNoAutoUppercase | Qt.ImhSensitiveData
                placeholderText: qsTr("github_pat_…")
                onAccepted: connectToken.click()
            }

            Label {
                Layout.fillWidth: true
                color: Theme.textFaint
                font.pointSize: Theme.fontSizeSmall
                text: qsTr("Needs read and write access to pull requests and Actions. It is kept in the system keyring.")
                wrapMode: Text.Wrap
            }
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.spacingL

            // The ghost style: an outline and no fill.
            FlatButton {
                id: connectToken

                objectName: "githubConnectToken"
                Layout.fillWidth: true
                enabled: root.idle
                font.pointSize: Theme.fontSizeBase
                implicitHeight: Theme.fieldHeight
                text: qsTr("Connect with the token")
                onClicked: Github.connectWithToken(token.text)

                background: Rectangle {
                    border.color: Theme.raisedHover
                    border.width: Theme.borderWidth
                    color: connectToken.hovered ? Theme.raisedHover : Theme.transparent
                    radius: Theme.radiusControl
                }
            }

            BusyIndicator {
                objectName: "githubConnectBusy"
                Layout.preferredHeight: Theme.busyIndicatorSize
                Layout.preferredWidth: Theme.busyIndicatorSize
                palette.dark: Theme.textMuted
                running: visible
                visible: !root.idle
            }
        }

        Label {
            objectName: "githubConnectError"
            Layout.fillWidth: true
            color: Theme.error
            font.pointSize: Theme.fontSizeSmall
            text: Github.connectReason
            visible: text !== ""
            wrapMode: Text.Wrap
        }
    }
}

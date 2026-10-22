// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

#pragma once

#include <QObject>
#include <QString>
#include <QtQml/qqmlregistration.h>

/// The system clipboard, which QML has no way to reach.
class Clipboard : public QObject
{
    Q_OBJECT
    QML_ELEMENT
    QML_SINGLETON

public:
    using QObject::QObject;

    /// Replaces what the clipboard holds with `text`.
    Q_INVOKABLE void setText(const QString &text);
};

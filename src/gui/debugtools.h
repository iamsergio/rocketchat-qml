// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

#pragma once

#include <QObject>
#include <QtQml/qqmlregistration.h>

/// Dumps what helps to diagnose a user's problem to the log, from the Settings menu.
class DebugTools : public QObject
{
    Q_OBJECT
    QML_ELEMENT
    QML_SINGLETON

public:
    using QObject::QObject;

    /// Logs the sections of information below, one after the other. For now only the DPI one.
    Q_INVOKABLE void dumpInfo();
};

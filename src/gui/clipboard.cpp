// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

#include "clipboard.h"

#include <QClipboard>
#include <QGuiApplication>

void Clipboard::setText(const QString &text)
{
    if (QClipboard *clipboard = QGuiApplication::clipboard())
        clipboard->setText(text);
}

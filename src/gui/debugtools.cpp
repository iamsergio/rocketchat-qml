// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

#include "debugtools.h"

#include "kdtoolbox/dpiinfo.h"

#include <QDebug>

void DebugTools::dumpInfo()
{
    qInfo().noquote().nospace() << "DPI:\n" << KDToolBox::hdpiInfo();
}

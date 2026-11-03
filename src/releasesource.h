// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include "releaseconfig.h"
#include <QByteArray>
#include <QString>
#include <QUrl>

namespace ReleaseSource {
QUrl latestApi(const QString &forge = QStringLiteral(PVS_RELEASE_FORGE));
QUrl releasePage(const QByteArray &json, const QString &forge = QStringLiteral(PVS_RELEASE_FORGE));
}

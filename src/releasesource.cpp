// SPDX-License-Identifier: GPL-3.0-or-later
#include "releasesource.h"
#include <QJsonDocument>
#include <QJsonObject>

QUrl ReleaseSource::latestApi(const QString &forge)
{
    if (forge == QStringLiteral("gitlab")) {
        const QByteArray project = QUrl::toPercentEncoding(
            QStringLiteral("stantonmatthewj/plasma-wayland-screensaver"));
        return QUrl(QStringLiteral("https://gitlab.com/api/v4/projects/%1/releases/permalink/latest")
                        .arg(QString::fromLatin1(project)));
    }
    if (forge == QStringLiteral("github")) {
        return QUrl(QStringLiteral("https://api.github.com/repos/StantonMatt/plasma-wayland-screensaver/releases/latest"));
    }
    return {};
}

QUrl ReleaseSource::releasePage(const QByteArray &json, const QString &forge)
{
    const auto object = QJsonDocument::fromJson(json).object();
    QUrl url;
    QString host;
    if (forge == QStringLiteral("gitlab")) {
        url = QUrl(object.value(QStringLiteral("_links")).toObject().value(QStringLiteral("self")).toString());
        host = QStringLiteral("gitlab.com");
    } else if (forge == QStringLiteral("github")) {
        url = QUrl(object.value(QStringLiteral("html_url")).toString());
        host = QStringLiteral("github.com");
    }
    if (url.scheme() != QStringLiteral("https") || url.host() != host || !url.userInfo().isEmpty()) {
        return {};
    }
    return url;
}

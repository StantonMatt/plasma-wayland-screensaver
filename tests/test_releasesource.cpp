// SPDX-License-Identifier: GPL-3.0-or-later
#include "releasesource.h"
#include <QTest>

class ReleaseSourceTest final : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void forgeEndpoints()
    {
        QCOMPARE(ReleaseSource::latestApi(QStringLiteral("gitlab")).toEncoded(), QByteArray(
            "https://gitlab.com/api/v4/projects/88430057/releases/permalink/latest"));
        QCOMPARE(ReleaseSource::latestApi(QStringLiteral("github")), QUrl(QStringLiteral(
            "https://api.github.com/repos/StantonMatt/plasma-wayland-screensaver/releases/latest")));
        QVERIFY(ReleaseSource::latestApi(QStringLiteral("unknown")).isEmpty());
        QCOMPARE(ReleaseSource::latestApi(), ReleaseSource::latestApi(QStringLiteral(PVS_RELEASE_FORGE)));
    }

    void response_data()
    {
        QTest::addColumn<QString>("forge");
        QTest::addColumn<QByteArray>("json");
        QTest::addColumn<QString>("expected");
        QTest::newRow("gitlab") << QStringLiteral("gitlab")
            << QByteArray("{\"_links\":{\"self\":\"https://gitlab.com/StantonMatt/plasma-wayland-screensaver/-/releases/v0.21.0\"}}")
            << QStringLiteral("https://gitlab.com/StantonMatt/plasma-wayland-screensaver/-/releases/v0.21.0");
        QTest::newRow("github") << QStringLiteral("github")
            << QByteArray("{\"html_url\":\"https://github.com/StantonMatt/plasma-wayland-screensaver/releases/tag/v0.21.0\"}")
            << QStringLiteral("https://github.com/StantonMatt/plasma-wayland-screensaver/releases/tag/v0.21.0");
        QTest::newRow("invalid-json") << QStringLiteral("gitlab") << QByteArray("broken") << QString();
        QTest::newRow("missing-link") << QStringLiteral("gitlab") << QByteArray("{}") << QString();
        QTest::newRow("wrong-forge") << QStringLiteral("gitlab")
            << QByteArray("{\"html_url\":\"https://github.com/releases\"}") << QString();
        QTest::newRow("unsafe-scheme") << QStringLiteral("gitlab")
            << QByteArray("{\"_links\":{\"self\":\"file:///etc/passwd\"}}") << QString();
        QTest::newRow("wrong-host") << QStringLiteral("gitlab")
            << QByteArray("{\"_links\":{\"self\":\"https://example.com/releases\"}}") << QString();
    }

    void response()
    {
        QFETCH(QString, forge);
        QFETCH(QByteArray, json);
        QFETCH(QString, expected);
        QCOMPARE(ReleaseSource::releasePage(json, forge), QUrl(expected));
    }
};
QTEST_GUILESS_MAIN(ReleaseSourceTest)
#include "test_releasesource.moc"

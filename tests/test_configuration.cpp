// SPDX-License-Identifier: GPL-3.0-or-later
#include "../src/configuration.h"

#include <KConfig>
#include <KConfigGroup>
#include <QTemporaryDir>
#include <QTest>

class ConfigurationTest final : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void defaultsAndValidation()
    {
        QTemporaryDir directory;
        QVERIFY(directory.isValid());
        Configuration config(directory.filePath(QStringLiteral("settingsrc")));
        QCOMPARE(config.idleMinutes(), 10);
        QCOMPARE(config.visualModule(), QStringLiteral("aurora"));
        QCOMPARE(config.backgroundStyle(), QStringLiteral("midnight"));
        QCOMPARE(config.clockMovement(), QStringLiteral("bounce"));
        QCOMPARE(config.clockSpeed(), QStringLiteral("normal"));
        QCOMPARE(config.coverPanels(), true);

        config.setIdleMinutes(0);
        config.setFrameRate(42);
        config.setVisualModule(QStringLiteral("not-installed"));
        config.setBackgroundStyle(QStringLiteral("invalid"));
        config.setClockMovement(QStringLiteral("invalid"));
        config.setClockSpeed(QStringLiteral("invalid"));
        config.setMonitorBehavior(QStringLiteral("invalid"));
        QCOMPARE(config.idleMinutes(), 1);
        QCOMPARE(config.frameRate(), 60);
        QCOMPARE(config.visualModule(), QStringLiteral("aurora"));
        QCOMPARE(config.backgroundStyle(), QStringLiteral("midnight"));
        QCOMPARE(config.clockMovement(), QStringLiteral("bounce"));
        QCOMPARE(config.clockSpeed(), QStringLiteral("normal"));
        QCOMPARE(config.monitorBehavior(), QStringLiteral("independent"));
    }

    void roundTrip()
    {
        QTemporaryDir directory;
        const QString path = directory.filePath(QStringLiteral("settingsrc"));
        {
            Configuration config(path);
            config.setIdleMinutes(27);
            config.setVisualModule(QStringLiteral("bounce"));
            config.setBackgroundStyle(QStringLiteral("plum"));
            config.setShowClock(false);
            config.setClockMovement(QStringLiteral("center"));
            config.setClockSpeed(QStringLiteral("fast"));
            config.setFrameRate(15);
            config.setReducedMotion(true);
            config.setMonitorBehavior(QStringLiteral("seamless"));
            config.setCoverPanels(false);
            config.save();
        }
        Configuration loaded(path);
        QCOMPARE(loaded.idleMinutes(), 27);
        QCOMPARE(loaded.visualModule(), QStringLiteral("bounce"));
        QCOMPARE(loaded.backgroundStyle(), QStringLiteral("plum"));
        QCOMPARE(loaded.showClock(), false);
        QCOMPARE(loaded.clockMovement(), QStringLiteral("center"));
        QCOMPARE(loaded.clockSpeed(), QStringLiteral("fast"));
        QCOMPARE(loaded.frameRate(), 15);
        QCOMPARE(loaded.reducedMotion(), true);
        QCOMPARE(loaded.monitorBehavior(), QStringLiteral("seamless"));
        QCOMPARE(loaded.coverPanels(), false);
    }

    void migratesCombinedBlackVisual()
    {
        QTemporaryDir directory;
        const QString path = directory.filePath(QStringLiteral("settingsrc"));
        {
            KConfig raw(path, KConfig::SimpleConfig);
            KConfigGroup general(&raw, QStringLiteral("General"));
            general.writeEntry("VisualModule", QStringLiteral("black"));
            raw.sync();
        }
        Configuration loaded(path);
        QCOMPARE(loaded.visualModule(), QStringLiteral("none"));
        QCOMPARE(loaded.backgroundStyle(), QStringLiteral("black"));
    }
};

QTEST_GUILESS_MAIN(ConfigurationTest)
#include "test_configuration.moc"

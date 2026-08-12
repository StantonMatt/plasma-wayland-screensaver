// SPDX-License-Identifier: GPL-3.0-or-later
#include "../src/configuration.h"

#include <KConfig>
#include <KConfigGroup>
#include <QTemporaryDir>
#include <QTest>

#include <limits>

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
        QCOMPARE(config.animationSpeed(), 100);
        QCOMPARE(config.animationDensity(), 50);
        QCOMPARE(config.animationScale(), 100);
        QCOMPARE(config.animationPalette(), QStringLiteral("ocean"));
        QCOMPARE(config.trailAmount(), 35);
        QCOMPARE(config.ballCount(), 5);
        QCOMPARE(config.ballGravity(), 35);
        QCOMPARE(config.ballElasticity(), 92);
        QCOMPARE(config.ballCollisions(), true);
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
        config.setAnimationSpeed(999);
        config.setAnimationDensity(0);
        config.setAnimationScale(1);
        config.setAnimationPalette(QStringLiteral("invalid"));
        config.setTrailAmount(-5);
        config.setBallCount(99);
        config.setBallGravity(-999);
        config.setBallElasticity(2);
        QCOMPARE(config.idleMinutes(), 1);
        QCOMPARE(config.frameRate(), 45);
        QCOMPARE(config.visualModule(), QStringLiteral("aurora"));
        QCOMPARE(config.backgroundStyle(), QStringLiteral("midnight"));
        QCOMPARE(config.clockMovement(), QStringLiteral("bounce"));
        QCOMPARE(config.clockSpeed(), QStringLiteral("normal"));
        QCOMPARE(config.monitorBehavior(), QStringLiteral("independent"));
        QCOMPARE(config.animationSpeed(), 300);
        QCOMPARE(config.animationDensity(), 10);
        QCOMPARE(config.animationScale(), 25);
        QCOMPARE(config.animationPalette(), QStringLiteral("ocean"));
        QCOMPARE(config.trailAmount(), 0);
        QCOMPARE(config.ballCount(), 20);
        QCOMPARE(config.ballGravity(), -100);
        QCOMPARE(config.ballElasticity(), 50);
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
            config.setAnimationSpeed(180);
            config.setAnimationDensity(75);
            config.setAnimationScale(135);
            config.setAnimationPalette(QStringLiteral("ember"));
            config.setTrailAmount(80);
            config.setBallCount(12);
            config.setBallGravity(-40);
            config.setBallElasticity(76);
            config.setBallCollisions(false);
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
        QCOMPARE(loaded.animationSpeed(), 180);
        QCOMPARE(loaded.animationDensity(), 75);
        QCOMPARE(loaded.animationScale(), 135);
        QCOMPARE(loaded.animationPalette(), QStringLiteral("ember"));
        QCOMPARE(loaded.trailAmount(), 80);
        QCOMPARE(loaded.ballCount(), 12);
        QCOMPARE(loaded.ballGravity(), -40);
        QCOMPARE(loaded.ballElasticity(), 76);
        QCOMPARE(loaded.ballCollisions(), false);
        QCOMPARE(loaded.showClock(), false);
        QCOMPARE(loaded.clockMovement(), QStringLiteral("center"));
        QCOMPARE(loaded.clockSpeed(), QStringLiteral("fast"));
        QCOMPARE(loaded.frameRate(), 15);
        QCOMPARE(loaded.reducedMotion(), true);
        QCOMPARE(loaded.monitorBehavior(), QStringLiteral("seamless"));
        QCOMPARE(loaded.coverPanels(), false);
    }

    void acceptsAllBundledVisualModules()
    {
        QTemporaryDir directory;
        Configuration config(directory.filePath(QStringLiteral("settingsrc")));
        const QStringList modules = {
            QStringLiteral("none"), QStringLiteral("aurora"),
            QStringLiteral("orbs"), QStringLiteral("bounce"),
            QStringLiteral("starfield"), QStringLiteral("matrix"),
            QStringLiteral("kaleidoscope"), QStringLiteral("fireflies"),
            QStringLiteral("ribbons"), QStringLiteral("constellation"),
        };
        for (const QString &module : modules) {
            config.setVisualModule(module);
            QCOMPARE(config.visualModule(), module);
        }
    }

    void supportsExpandedAndAutomaticFrameRates()
    {
        QTemporaryDir directory;
        Configuration config(directory.filePath(QStringLiteral("settingsrc")));
        const QList<int> rates = {0, 15, 24, 30, 45, 60, 75, 90, 100,
                                  120, 144, 165, 175, 200, 240};
        for (int rate : rates) {
            config.setFrameRate(rate);
            QCOMPARE(config.frameRate(), rate);
        }
        config.setFrameRate(239);
        QCOMPARE(config.frameRate(), 240);
        config.setFrameRate(-1);
        QCOMPARE(config.frameRate(), 15);
        config.setFrameRate(std::numeric_limits<int>::min());
        QCOMPARE(config.frameRate(), 15);
        config.setFrameRate(5);
        QCOMPARE(config.frameRate(), 15);
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

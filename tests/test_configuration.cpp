// SPDX-License-Identifier: GPL-3.0-or-later
#include "../src/configuration.h"

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

        config.setIdleMinutes(0);
        config.setFrameRate(42);
        config.setVisualModule(QStringLiteral("not-installed"));
        config.setMonitorBehavior(QStringLiteral("invalid"));
        QCOMPARE(config.idleMinutes(), 1);
        QCOMPARE(config.frameRate(), 60);
        QCOMPARE(config.visualModule(), QStringLiteral("aurora"));
        QCOMPARE(config.monitorBehavior(), QStringLiteral("independent"));
    }

    void roundTrip()
    {
        QTemporaryDir directory;
        const QString path = directory.filePath(QStringLiteral("settingsrc"));
        {
            Configuration config(path);
            config.setIdleMinutes(27);
            config.setVisualModule(QStringLiteral("orbs"));
            config.setShowClock(false);
            config.setFrameRate(15);
            config.setReducedMotion(true);
            config.setMonitorBehavior(QStringLiteral("synchronized"));
            config.save();
        }
        Configuration loaded(path);
        QCOMPARE(loaded.idleMinutes(), 27);
        QCOMPARE(loaded.visualModule(), QStringLiteral("orbs"));
        QCOMPARE(loaded.showClock(), false);
        QCOMPARE(loaded.frameRate(), 15);
        QCOMPARE(loaded.reducedMotion(), true);
        QCOMPARE(loaded.monitorBehavior(), QStringLiteral("synchronized"));
    }
};

QTEST_GUILESS_MAIN(ConfigurationTest)
#include "test_configuration.moc"

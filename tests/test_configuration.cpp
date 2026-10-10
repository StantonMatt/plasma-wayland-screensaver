// SPDX-License-Identifier: GPL-3.0-or-later
#include "../src/configuration.h"

#include <KConfig>
#include <KConfigGroup>
#include <QSignalSpy>
#include <QFile>
#include <QTemporaryDir>
#include <QTest>

#include <limits>

class ConfigurationTest final : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void seasonalAppearanceRoundTrip()
    {
        QTemporaryDir dir;
        const auto path = dir.filePath(QStringLiteral("settingsrc"));
        Configuration config(path);
        QVERIFY(config.seasonalThemes());
        QCOMPARE(config.defaults(QStringLiteral("appearance")).value(QStringLiteral("seasonalThemes")).toBool(), true);
        QVERIFY(!config.defaults(QStringLiteral("general")).contains(QStringLiteral("seasonalThemes")));
        const auto original = config.snapshot(QStringLiteral("appearance"));
        config.apply({{QStringLiteral("seasonalThemes"), false}});
        QVERIFY(!config.seasonalThemes());
        config.save();
        Configuration restored(path);
        QVERIFY(!restored.seasonalThemes());
        KConfig disk(path, KConfig::SimpleConfig);
        QVERIFY(!KConfigGroup(&disk, QStringLiteral("General")).readEntry("SeasonalThemes", true));
        restored.restoreDefaults(QStringLiteral("general"));
        QVERIFY(!restored.seasonalThemes());
        restored.restoreDefaults(QStringLiteral("appearance"));
        QVERIFY(restored.seasonalThemes());
        config.apply(original);
        QVERIFY(config.seasonalThemes());
        config.setVisualModule(QStringLiteral("snakes"));
        config.setSeasonalThemes(false);
        config.setVisualModule(QStringLiteral("aurora"));
        QVERIFY(!config.seasonalThemes()); // global appearance key, not an animation profile
    }
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
        QCOMPARE(config.snakeIntelligence(), 75);
        QCOMPARE(config.snakeAggression(), 100);
        QCOMPARE(config.snakeSelfCollisions(), false);
        QCOMPARE(config.snakeDeadlyWalls(), true);
        QCOMPARE(config.snakeLengthLimit(), false);
        QCOMPARE(config.snakePowerUps(), true);
        QCOMPARE(config.snakeStorePowerUps(), true);
        QCOMPARE(config.snakeWorldEvents(), true);
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
        config.setSnakeIntelligence(999);
        config.setSnakeAggression(999);
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
        QCOMPARE(config.snakeIntelligence(), 100);
        QCOMPARE(config.snakeAggression(), 100);
        config.setSnakeAggression(-1);
        QCOMPARE(config.snakeAggression(), 0);
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
            config.setSnakeIntelligence(90);
            config.setSnakeAggression(50);
            config.setSnakeSelfCollisions(true);
            config.setSnakeDeadlyWalls(false);
            config.setSnakeLengthLimit(true);
            config.setSnakePowerUps(false);
            config.setSnakeStorePowerUps(false);
            config.setSnakeWorldEvents(false);
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
        QCOMPARE(loaded.snakeIntelligence(), 90);
        QCOMPARE(loaded.snakeAggression(), 50);
        QCOMPARE(loaded.snakeSelfCollisions(), true);
        QCOMPARE(loaded.snakeDeadlyWalls(), false);
        QCOMPARE(loaded.snakeLengthLimit(), true);
        QCOMPARE(loaded.snakePowerUps(), false);
        QCOMPARE(loaded.snakeStorePowerUps(), false);
        QCOMPARE(loaded.snakeWorldEvents(), false);
        QCOMPARE(loaded.showClock(), false);
        QCOMPARE(loaded.clockMovement(), QStringLiteral("center"));
        QCOMPARE(loaded.clockSpeed(), QStringLiteral("fast"));
        QCOMPARE(loaded.frameRate(), 15);
        QCOMPARE(loaded.reducedMotion(), true);
        QCOMPARE(loaded.monitorBehavior(), QStringLiteral("seamless"));
        QCOMPARE(loaded.coverPanels(), false);
    }

    void appliesSettingsAtomically()
    {
        QTemporaryDir directory;
        QVERIFY(directory.isValid());
        Configuration config(directory.filePath(QStringLiteral("settingsrc")));
        QSignalSpy changed(&config, &Configuration::changed);

        config.apply({
            {QStringLiteral("idleMinutes"), 42},
            {QStringLiteral("visualModule"), QStringLiteral("bounce")},
            {QStringLiteral("backgroundStyle"), QStringLiteral("black")},
            {QStringLiteral("animationSpeed"), 170},
            {QStringLiteral("ballCount"), 11},
            {QStringLiteral("snakeIntelligence"), 85},
            {QStringLiteral("snakeAggression"), 50},
            {QStringLiteral("snakeSelfCollisions"), true},
            {QStringLiteral("snakeDeadlyWalls"), false},
            {QStringLiteral("snakeLengthLimit"), true},
            {QStringLiteral("snakePowerUps"), false},
            {QStringLiteral("snakeStorePowerUps"), false},
            {QStringLiteral("snakeWorldEvents"), false},
            {QStringLiteral("showClock"), false},
            {QStringLiteral("clockSpeed"), QStringLiteral("fast")},
            {QStringLiteral("frameRate"), 120},
            {QStringLiteral("monitorBehavior"), QStringLiteral("seamless")},
            {QStringLiteral("coverPanels"), false},
        });

        QCOMPARE(changed.count(), 1);
        QCOMPARE(config.idleMinutes(), 42);
        QCOMPARE(config.visualModule(), QStringLiteral("bounce"));
        QCOMPARE(config.backgroundStyle(), QStringLiteral("black"));
        QCOMPARE(config.animationSpeed(), 170);
        QCOMPARE(config.ballCount(), 11);
        QCOMPARE(config.snakeIntelligence(), 85);
        QCOMPARE(config.snakeAggression(), 50);
        QCOMPARE(config.snakeSelfCollisions(), true);
        QCOMPARE(config.snakeDeadlyWalls(), false);
        QCOMPARE(config.snakeLengthLimit(), true);
        QCOMPARE(config.snakePowerUps(), false);
        QCOMPARE(config.snakeStorePowerUps(), false);
        QCOMPARE(config.snakeWorldEvents(), false);
        QCOMPARE(config.showClock(), false);
        QCOMPARE(config.clockSpeed(), QStringLiteral("fast"));
        QCOMPARE(config.frameRate(), 120);
        QCOMPARE(config.monitorBehavior(), QStringLiteral("seamless"));
        QCOMPARE(config.coverPanels(), false);

        changed.clear();
        config.apply({
            {QStringLiteral("idleMinutes"), 42},
            {QStringLiteral("ballCount"), 11},
        });
        QCOMPARE(changed.count(), 0);
        QCOMPARE(config.visualModule(), QStringLiteral("bounce"));
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
            QStringLiteral("snakes"),
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

    void migratesRealSharedConfigurationOnce()
    {
        QTemporaryDir directory;
        const QString path = directory.filePath(QStringLiteral("settingsrc"));
        const QString fixture = QFINDTESTDATA("fixtures/settings-pre-animation-profiles.rc");
        QVERIFY(!fixture.isEmpty());
        QVERIFY(QFile::copy(fixture, path));
        Configuration config(path);
        QCOMPARE(config.visualModule(), QStringLiteral("snakes"));
        QCOMPARE(config.idleMinutes(), 1);
        QCOMPARE(config.backgroundStyle(), QStringLiteral("black"));
        QCOMPARE(config.monitorBehavior(), QStringLiteral("seamless"));
        QCOMPARE(config.clockMode(), 4);
        QCOMPARE(config.ballCount(), 20);
        QCOMPARE(config.snakeIntelligence(), 100);
        const QVariantMap profiles = config.snapshot().value(QStringLiteral("animationSettings")).toMap();
        QCOMPARE(profiles.size(), 11);
        for (auto it = profiles.cbegin(); it != profiles.cend(); ++it) {
            config.setVisualModule(it.key());
            QCOMPARE(config.animationSpeed(), 300);
            QCOMPARE(config.animationDensity(), 80);
            QCOMPARE(config.animationScale(), 200);
            QCOMPARE(config.trailAmount(), 100);
            QCOMPARE(config.animationPalette(), QStringLiteral("ember"));
        }
        // Migration was persisted before any explicit save.
        KConfig migrated(path, KConfig::SimpleConfig);
        QCOMPARE(KConfigGroup(&migrated, QStringLiteral("General")).readEntry("AnimationSettingsVersion", 0), 1);
        config.setVisualModule(QStringLiteral("aurora"));
        config.setAnimationSpeed(120);
        config.setAnimationPalette(QStringLiteral("pastel"));
        config.save();
        // Old shared keys must never seed the profiles a second time.
        KConfigGroup general(&migrated, QStringLiteral("General"));
        general.writeEntry("AnimationSpeed", 10);
        migrated.sync();
        config.reload();
        QCOMPARE(config.animationSpeed(), 120);
        QCOMPARE(config.animationPalette(), QStringLiteral("pastel"));
        config.setVisualModule(QStringLiteral("snakes"));
        QCOMPARE(config.animationSpeed(), 300);
        QCOMPARE(config.animationPalette(), QStringLiteral("ember"));
    }

    void perAnimationRoundTripsAndValidates()
    {
        QTemporaryDir directory;
        const QString path = directory.filePath(QStringLiteral("settingsrc"));
        Configuration config(path);
        const QStringList modules = config.defaults().value(QStringLiteral("animationSettings")).toMap().keys();
        QVariantMap expected;
        int i = 0;
        for (const QString &module : modules) {
            config.apply({{QStringLiteral("visualModule"), module},
                {QStringLiteral("animationSpeed"), 10 + i * 20},
                {QStringLiteral("animationDensity"), 10 + i * 5},
                {QStringLiteral("animationScale"), 25 + i * 10},
                {QStringLiteral("trailAmount"), i * 7},
                {QStringLiteral("animationPalette"), i % 2 ? QStringLiteral("ember") : QStringLiteral("mono")}});
            expected = config.snapshot();
            ++i;
        }
        config.save();
        Configuration loaded(path);
        QCOMPARE(loaded.snapshot(), expected);
        {
            KConfig raw(path, KConfig::SimpleConfig);
            KConfigGroup animations(&raw, QStringLiteral("Animations"));
            KConfigGroup snakes = animations.group(QStringLiteral("snakes"));
            snakes.writeEntry("AnimationSpeed", 999);
            snakes.writeEntry("AnimationDensity", -20);
            snakes.writeEntry("AnimationScale", 999);
            snakes.writeEntry("TrailAmount", -10);
            snakes.writeEntry("AnimationPalette", QStringLiteral("unknown"));
            // A missing profile after migration uses defaults, not the shared keys.
            animations.group(QStringLiteral("orbs")).deleteGroup();
            raw.sync();
        }
        loaded.reload();
        loaded.setVisualModule(QStringLiteral("snakes"));
        QCOMPARE(loaded.animationSpeed(), 300);
        QCOMPARE(loaded.animationDensity(), 10);
        QCOMPARE(loaded.animationScale(), 200);
        QCOMPARE(loaded.trailAmount(), 0);
        QCOMPARE(loaded.animationPalette(), QStringLiteral("ocean"));
        loaded.setVisualModule(QStringLiteral("orbs"));
        QCOMPARE(loaded.animationSpeed(), 100);
        QCOMPARE(loaded.animationDensity(), 50);
        QCOMPARE(loaded.animationScale(), 100);
        QCOMPARE(loaded.trailAmount(), 35);
        QCOMPARE(loaded.animationPalette(), QStringLiteral("ocean"));
    }

    void pageDefaultsAndUndoRestoreAllProfiles()
    {
        QTemporaryDir directory;
        Configuration config(directory.filePath(QStringLiteral("settingsrc")));
        QCOMPARE(config.snapshot(), config.defaults());
        config.setAnimationSpeed(250);
        config.setVisualModule(QStringLiteral("snakes"));
        config.setAnimationDensity(80);
        config.setAnimationPalette(QStringLiteral("ember"));
        config.setSnakeAggression(45);
        config.setIdleMinutes(1);
        config.setFrameRate(144);
        const QVariantMap appearance = config.snapshot(QStringLiteral("appearance"));
        const QVariantMap general = config.snapshot(QStringLiteral("general"));
        const QVariantMap before = config.snapshot();
        QSignalSpy changed(&config, &Configuration::changed);
        config.restoreDefaults(QStringLiteral("appearance"));
        QCOMPARE(changed.count(), 1);
        QCOMPARE(config.snapshot(QStringLiteral("appearance")), config.defaults(QStringLiteral("appearance")));
        QCOMPARE(config.snapshot(QStringLiteral("general")), general);
        changed.clear();
        config.apply(appearance);
        QCOMPARE(changed.count(), 1);
        QCOMPARE(config.snapshot(), before);
        config.restoreDefaults(QStringLiteral("general"));
        QCOMPARE(config.snapshot(QStringLiteral("appearance")), appearance);
        QCOMPARE(config.snapshot(QStringLiteral("general")), config.defaults(QStringLiteral("general")));
        config.apply(general);
        QCOMPARE(config.snapshot(), before);
        config.restoreDefaults(QStringLiteral("invalid"));
        QCOMPARE(config.snapshot(), before);
        config.restoreDefaults();
        QCOMPARE(config.snapshot(), config.defaults());
    }

    void clockModeMappingPreservesHiddenChoices()
    {
        QTemporaryDir directory;
        Configuration config(directory.filePath(QStringLiteral("settingsrc")));
        for (int mode : {2, 3, 4}) {
            QSignalSpy changed(&config, &Configuration::changed);
            config.setClockMode(mode);
            QCOMPARE(config.clockMode(), mode);
            QCOMPARE(config.showClock(), true);
            QCOMPARE(config.clockMovement(), QStringLiteral("bounce"));
            QCOMPARE(config.clockSpeed(), mode == 2 ? QStringLiteral("slow") : mode == 4 ? QStringLiteral("fast") : QStringLiteral("normal"));
            QVERIFY(changed.count() <= 1);
        }
        config.setClockMode(0);
        QCOMPARE(config.showClock(), false);
        QCOMPARE(config.clockSpeed(), QStringLiteral("fast"));
        QCOMPARE(config.clockMovement(), QStringLiteral("bounce"));
        config.setClockMode(1);
        QCOMPARE(config.clockMode(), 1);
        QCOMPARE(config.clockSpeed(), QStringLiteral("fast"));
        config.setClockMode(-10);
        QCOMPARE(config.clockMode(), 0);
        QCOMPARE(config.clockMovement(), QStringLiteral("center"));
        config.apply({{QStringLiteral("clockMode"), 99}});
        QCOMPARE(config.clockMode(), 4);
        config.setShowClock(false);
        QCOMPARE(config.clockMode(), 0);
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

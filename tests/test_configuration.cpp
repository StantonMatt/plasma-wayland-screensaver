// SPDX-License-Identifier: GPL-3.0-or-later
#include "../src/configuration.h"
#include "visualdepthregistry.h"
#include <QScopeGuard>

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
        QCOMPARE(config.ballCount(), 60);
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
        // Loading stages migrations in memory; only an explicit save writes.
        KConfig migrated(path, KConfig::SimpleConfig);
        QCOMPARE(KConfigGroup(&migrated, QStringLiteral("General")).readEntry("AnimationSettingsVersion", 0), 0);
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
            expected = config.snapshot({}, false);
            ++i;
        }
        config.save();
        Configuration loaded(path);
        QCOMPARE(loaded.snapshot({}, false), expected);
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
        QCOMPARE(config.snapshot({}, false), config.defaults());
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
        QCOMPARE(config.snapshot(QStringLiteral("appearance"), false), config.defaults(QStringLiteral("appearance")));
        QCOMPARE(config.snapshot(QStringLiteral("general")), general);
        changed.clear();
        config.apply(appearance);
        QCOMPARE(changed.count(), 1);
        QCOMPARE(config.snapshot(), before);
        config.restoreDefaults(QStringLiteral("general"));
        QCOMPARE(config.snapshot(QStringLiteral("appearance")), appearance);
        QCOMPARE(config.snapshot(QStringLiteral("general"), false), config.defaults(QStringLiteral("general")));
        config.apply(general);
        QCOMPARE(config.snapshot(), before);
        config.restoreDefaults(QStringLiteral("invalid"));
        QCOMPARE(config.snapshot(), before);
        config.restoreDefaults();
        QCOMPARE(config.snapshot({}, false), config.defaults());
    }

    void depthMigrationAndPersistence()
    {
        QTemporaryDir dir;
        const auto path = dir.filePath(QStringLiteral("settingsrc"));
        QFile file(path);
        QVERIFY(file.open(QIODevice::WriteOnly));
        const QByteArray input = "[General]\nAnimationSettingsVersion=1\nVisualModule=kaleidoscope\nBallCount=60\n\n["
                                 "Animations][kaleidoscope]\nAnimationSpeed=300\nAnimationDensity=80\nAnimationScale="
                                 "200\nAnimationPalette=ember\nTrailAmount=100\nUnrecognised=keep "
                                 "me\n\n[Animations][matrix]\nDepthSettingsVersion=9\nweather=future-weather\nwind="
                                 "333\nUnknownFuture=present\n";
        file.write(input);
        file.close();
        Configuration config(path);
        QVERIFY(file.open(QIODevice::ReadOnly));
        QCOMPARE(file.readAll(), input);
        file.close();
        QCOMPARE(config.visualParameter(QStringLiteral("points")).toInt(), 8);
        QCOMPARE(config.animationPalette(), QStringLiteral("ember"));
        config.setVisualParameter(QStringLiteral("turning"), 75);
        config.setVisualModule(QStringLiteral("constellation"));
        config.setVisualParameter(QStringLiteral("gravity"), 80);
        config.save();
        Configuration loaded(path);
        QCOMPARE(loaded.visualModule(), QStringLiteral("constellation"));
        QCOMPARE(loaded.visualParameter(QStringLiteral("gravity")).toInt(), 80);
        QCOMPARE(loaded.ballCount(), 60);
        loaded.setVisualModule(QStringLiteral("kaleidoscope"));
        QCOMPARE(loaded.visualParameter(QStringLiteral("turning")).toInt(), 75);
        QCOMPARE(loaded.visualParameter(QStringLiteral("points")).toInt(), 8);
        loaded.save();
        KConfig disk(path, KConfig::SimpleConfig);
        const auto animations = KConfigGroup(&disk, QStringLiteral("Animations"));
        const auto matrix = animations.group(QStringLiteral("matrix"));
        QCOMPARE(matrix.readEntry("DepthSettingsVersion", 0), 9);
        QCOMPARE(matrix.readEntry("weather", QString()), QStringLiteral("future-weather"));
        QCOMPARE(matrix.readEntry("wind", 0), 333);
        QCOMPARE(matrix.readEntry("UnknownFuture", QString()), QStringLiteral("present"));
        const auto kaleidoscope = animations.group(QStringLiteral("kaleidoscope"));
        QCOMPARE(kaleidoscope.readEntry("Unrecognised", QString()), QStringLiteral("keep me"));
        QCOMPARE(kaleidoscope.readEntry("AnimationDensity", 0), 80);
        QVERIFY(!kaleidoscope.hasGroup(QStringLiteral("LegacyShared")));
        QVERIFY(!kaleidoscope.hasGroup(QStringLiteral("DepthShared")));
        QVERIFY(!animations.group(QStringLiteral("bounce")).hasKey("count"));
        QVERIFY(!kaleidoscope.hasKey("points")); // disabled slots retain the raw legacy tuple for activation
    }
    void untouchedAndExplicitDefaults()
    {
        for (int version : { 0, 1 })
        {
            QTemporaryDir dir;
            const auto path = dir.filePath(QStringLiteral("settingsrc"));
            QFile f(path);
            QVERIFY(f.open(QIODevice::WriteOnly));
            f.write(QByteArray("[General]\nAnimationSettingsVersion=") + QByteArray::number(version)
                + "\nVisualModule=kaleidoscope\nBallCount=5\nBallElasticity=92\n");
            f.close();
            QFile original(path);
            QVERIFY(original.open(QIODevice::ReadOnly));
            const auto before = original.readAll();
            original.close();
            Configuration c(path);
            QCOMPARE(c.ballCount(), 5);
            QCOMPARE(c.ballElasticity(), 92);
            QCOMPARE(c.visualParameter(QStringLiteral("points")).toInt(), 8);
            c.reload();
            QFile same(path);
            QVERIFY(same.open(QIODevice::ReadOnly));
            QCOMPARE(same.readAll(), before);
            same.close();
            c.save();
            KConfig disk(path, KConfig::SimpleConfig);
            const auto k = KConfigGroup(&disk, QStringLiteral("Animations")).group(QStringLiteral("kaleidoscope"));
            QVERIFY(!k.hasGroup(QStringLiteral("DepthShared")));
            QVERIFY(!k.hasKey("DepthBehaviorVersion"));
            QCOMPARE(k.readEntry("AnimationPalette", QString()), QStringLiteral("ocean"));
            QCOMPARE(c.animationPalette(), QStringLiteral("ocean")); // legacy path still disabled
        }
    }
#ifdef PVS_ENABLE_VISUAL_CAPTURE
    void availabilityActivatesInactiveProfilesTogetherWithTheirDefaults() {
        VisualDepthRegistry::setAvailabilityForTest(QStringLiteral("kaleidoscope"),true);
        VisualDepthRegistry::setAvailabilityForTest(QStringLiteral("bounce"),true);
        const auto reset=qScopeGuard([]{VisualDepthRegistry::setAvailabilityForTest(QStringLiteral("kaleidoscope"),std::nullopt);VisualDepthRegistry::setAvailabilityForTest(QStringLiteral("bounce"),std::nullopt);});
        QTemporaryDir dir;const auto path=dir.filePath(QStringLiteral("settingsrc"));
        QFile file(path);QVERIFY(file.open(QIODevice::WriteOnly));file.write("[General]\nAnimationSettingsVersion=1\nVisualModule=aurora\n");file.close();
        Configuration c(path);QCOMPARE(c.visualModule(),QStringLiteral("aurora"));QCOMPARE(c.ballCount(),12);QCOMPARE(c.ballElasticity(),90);
        c.setVisualModule(QStringLiteral("kaleidoscope"));QCOMPARE(c.animationPalette(),QStringLiteral("spectrum"));QCOMPARE(c.visualParameter(QStringLiteral("points")).toInt(),8);
        c.restoreDefaults(QStringLiteral("appearance"));QCOMPARE(c.snapshot(QStringLiteral("appearance"), false),c.defaults(QStringLiteral("appearance")));
        c.save();c.reload();QCOMPARE(c.animationPalette(),QStringLiteral("spectrum"));QCOMPARE(c.ballCount(),12);
        const auto explicitPath=dir.filePath(QStringLiteral("explicitrc"));QFile explicitFile(explicitPath);QVERIFY(explicitFile.open(QIODevice::WriteOnly));explicitFile.write("[General]\nBallCount=5\nBallElasticity=92\n");explicitFile.close();
        Configuration explicitValues(explicitPath);QCOMPARE(explicitValues.ballCount(),12);QCOMPARE(explicitValues.ballElasticity(),90);
        const auto customPath=dir.filePath(QStringLiteral("customrc"));QFile custom(customPath);QVERIFY(custom.open(QIODevice::WriteOnly));custom.write("[General]\nAnimationSettingsVersion=1\nVisualModule=aurora\n\n[Animations][kaleidoscope]\nAnimationDensity=80\nAnimationScale=200\nAnimationSpeed=300\nAnimationPalette=ember\nTrailAmount=100\n");custom.close();
        Configuration customized(customPath);customized.setVisualModule(QStringLiteral("kaleidoscope"));QCOMPARE(customized.visualParameter(QStringLiteral("points")).toInt(),14);QCOMPARE(customized.animationPalette(),QStringLiteral("ember"));

    }
#endif
#ifdef PVS_ENABLE_VISUAL_CAPTURE
    void disabledProfileUsesCurrentLegacyTupleAtActivation() {
        for(bool resetLegacy:{false,true}) {
            QTemporaryDir dir;const auto path=dir.filePath(QStringLiteral("settingsrc"));
            Configuration legacy(path);legacy.setVisualModule(QStringLiteral("kaleidoscope"));
            legacy.setAnimationDensity(80);legacy.setAnimationSpeed(300);legacy.setAnimationScale(200);legacy.setAnimationPalette(QStringLiteral("ember"));legacy.setTrailAmount(100);legacy.save();
            legacy.setAnimationDensity(100);legacy.save();
            if(resetLegacy){legacy.restoreDefaults(QStringLiteral("appearance"));legacy.save();}
            {KConfig disk(path,KConfig::SimpleConfig);const auto profile=KConfigGroup(&disk,QStringLiteral("Animations")).group(QStringLiteral("kaleidoscope"));QVERIFY(!profile.hasKey("points"));QVERIFY(!profile.hasKey("DepthBehaviorVersion"));}
            VisualDepthRegistry::setAvailabilityForTest(QStringLiteral("kaleidoscope"),true);
            const auto reset=qScopeGuard([]{VisualDepthRegistry::setAvailabilityForTest(QStringLiteral("kaleidoscope"),std::nullopt);});
            Configuration active(path);QCOMPARE(active.visualParameter(QStringLiteral("points")).toInt(),resetLegacy?8:16);QCOMPARE(active.animationPalette(),resetLegacy?QStringLiteral("spectrum"):QStringLiteral("ember"));active.save();
            active.setAnimationSpeed(100);active.setAnimationDensity(50);active.setAnimationScale(100);active.setAnimationPalette(QStringLiteral("ocean"));active.setTrailAmount(35);active.save();active.reload();
            QCOMPARE(active.animationPalette(),QStringLiteral("spectrum")); // an exact old-default tuple receives the available visual defaults
            QCOMPARE(active.visualParameter(QStringLiteral("points")).toInt(),resetLegacy?8:16);
        }
    }
#endif
    void unrelatedSaveRetainsRawSharedValues() {
        QTemporaryDir dir;const auto path=dir.filePath(QStringLiteral("settingsrc"));
        QFile file(path);QVERIFY(file.open(QIODevice::WriteOnly));file.write("[General]\nAnimationSettingsVersion=1\nVisualModule=kaleidoscope\n\n[Animations][kaleidoscope]\nAnimationDensity=80\nAnimationScale=999\nAnimationPalette=ember\n");file.close();
        Configuration c(path);QCOMPARE(c.animationScale(),200);c.save();
        KConfig disk(path,KConfig::SimpleConfig);const auto profile=KConfigGroup(&disk,QStringLiteral("Animations")).group(QStringLiteral("kaleidoscope"));
        QCOMPARE(profile.readEntry("AnimationScale",0),999);QVERIFY(!profile.hasGroup(QStringLiteral("LegacyShared")));
    }
    void appearanceDefaultsDoNotWriteBallPresenceMarkers()
    {
        QTemporaryDir dir;
        const auto path = dir.filePath(QStringLiteral("settingsrc"));
        Configuration c(path);
        c.setVisualModule(QStringLiteral("constellation"));
        c.restoreDefaults(QStringLiteral("appearance"));
        c.save();
        KConfig disk(path, KConfig::SimpleConfig);
        const auto bounce = KConfigGroup(&disk, QStringLiteral("Animations")).group(QStringLiteral("bounce"));
        QVERIFY(!bounce.hasKey("LegacyGeneralBallKeys"));
        QVERIFY(!bounce.hasKey("DepthBehaviorVersion"));
    }
    void appearanceDefaultsPreserveOtherVisuals()
    {
        QTemporaryDir dir;
        Configuration c(dir.filePath(QStringLiteral("settingsrc")));
        c.setVisualModule(QStringLiteral("snakes"));
        c.setAnimationSpeed(300);
        c.setSnakeAggression(40);
        c.setVisualModule(QStringLiteral("constellation"));
        c.setVisualParameter(QStringLiteral("gravity"),true);QCOMPARE(c.visualParameter(QStringLiteral("gravity")).toInt(),40);
        c.setVisualParameter(QStringLiteral("gravity"), 80);
        c.setAnimationDensity(80);
        c.restoreDefaults(QStringLiteral("appearance"));
        QCOMPARE(c.visualModule(), QStringLiteral("constellation"));
        QCOMPARE(c.visualParameter(QStringLiteral("gravity")).toInt(), 40);
        QCOMPARE(c.animationDensity(), 50);
        c.setVisualModule(QStringLiteral("snakes"));
        QCOMPARE(c.animationSpeed(), 300);
        QCOMPARE(c.snakeAggression(), 40);
    }

    void defaultsUndoPreservesRawParameters()
    {
        for (const bool saveDefaults : {false, true}) {
            QTemporaryDir dir;
            const auto path = dir.filePath(QStringLiteral("settingsrc"));
            QFile file(path);
            QVERIFY(file.open(QIODevice::WriteOnly));
            file.write("[General]\nVisualModule=matrix\nAnimationSettingsVersion=1\n\n"
                       "[Animations][matrix]\nDepthSettingsVersion=9\nwind=333\nweather=future-weather\nUnknownFuture=keep\n");
            file.close();
            Configuration config(path);
            config.setVisualParameter(QStringLiteral("glyphs"), QStringLiteral("numbers"));
            const auto before = config.snapshot(QStringLiteral("appearance"));
            config.restoreDefaults(QStringLiteral("appearance"));
            if (saveDefaults) config.save();
            config.apply(before);
            QCOMPARE(config.snapshot(QStringLiteral("appearance")), before);
            config.save();
            KConfig disk(path, KConfig::SimpleConfig);
            const auto matrix = KConfigGroup(&disk, QStringLiteral("Animations")).group(QStringLiteral("matrix"));
            QCOMPARE(matrix.readEntry("wind", QString()), QStringLiteral("333"));
            QCOMPARE(matrix.readEntry("weather", QString()), QStringLiteral("future-weather"));
            QCOMPARE(matrix.readEntry("UnknownFuture", QString()), QStringLiteral("keep"));
            QCOMPARE(matrix.readEntry("DepthSettingsVersion", 0), 9);
            QCOMPARE(matrix.readEntry("glyphs", QString()), QStringLiteral("numbers"));
        }
    }
#ifdef PVS_ENABLE_VISUAL_CAPTURE
    void disabledUnrelatedSaveRetainsActivatedProfile()
    {
        const auto reset = qScopeGuard([] { VisualDepthRegistry::setAvailabilityForTest(QStringLiteral("kaleidoscope"), std::nullopt); });
        QTemporaryDir dir;
        const auto path = dir.filePath(QStringLiteral("settingsrc"));
        { Configuration disabled(path); disabled.save(); }
        VisualDepthRegistry::setAvailabilityForTest(QStringLiteral("kaleidoscope"), true);
        {
            Configuration active(path);
            active.setVisualModule(QStringLiteral("kaleidoscope"));
            QCOMPARE(active.animationPalette(), QStringLiteral("spectrum"));
            active.save();
        }
        VisualDepthRegistry::setAvailabilityForTest(QStringLiteral("kaleidoscope"), false);
        {
            Configuration disabled(path);
            disabled.setIdleMinutes(27);
            disabled.save();
        }
        VisualDepthRegistry::setAvailabilityForTest(QStringLiteral("kaleidoscope"), true);
        Configuration upgraded(path);
        QCOMPARE(upgraded.animationPalette(), QStringLiteral("spectrum"));
    }

    void releasedBallEditsSurviveReupgrade()
    {
        const auto reset = qScopeGuard([] { VisualDepthRegistry::setAvailabilityForTest(QStringLiteral("bounce"), std::nullopt); });
        QTemporaryDir dir;
        const auto path = dir.filePath(QStringLiteral("settingsrc"));
        { Configuration disabled(path); disabled.save(); }
        {
            KConfig disk(path, KConfig::SimpleConfig);
            auto general = KConfigGroup(&disk, QStringLiteral("General"));
            general.writeEntry("BallCount", 17);
            general.writeEntry("BallElasticity", 77);
            auto bounce = KConfigGroup(&disk, QStringLiteral("Animations")).group(QStringLiteral("bounce"));
            bounce.writeEntry("LegacyGeneralBallKeys", QStringList()); // stale experimental metadata must be ignored
            QVERIFY(disk.sync());
        }
        VisualDepthRegistry::setAvailabilityForTest(QStringLiteral("bounce"), true);
        Configuration upgraded(path);
        QCOMPARE(upgraded.ballCount(), 17);
        QCOMPARE(upgraded.ballElasticity(), 77);
        upgraded.setVisualModule(QStringLiteral("bounce"));
        upgraded.setVisualParameter(QStringLiteral("count"), 60);
        upgraded.setVisualModule(QStringLiteral("matrix"));
        upgraded.save();
        upgraded.reload();
        upgraded.setVisualModule(QStringLiteral("bounce"));
        QCOMPARE(upgraded.visualParameter(QStringLiteral("count")).toInt(), 60);
    }

    void bounceUpgradesOnlyTheWholeReleasedDefaultTuple()
    {
        VisualDepthRegistry::setAvailabilityForTest(QStringLiteral("bounce"), true);
        const auto reset = qScopeGuard([] { VisualDepthRegistry::setAvailabilityForTest(QStringLiteral("bounce"), std::nullopt); });
        struct BallCase { QByteArray entry; int count; int elasticity; };
        for (const auto& test : {BallCase {"", 12, 90}, BallCase {"BallCount=17\n", 17, 92},
                 BallCase {"BallElasticity=77\n", 5, 77}, BallCase {"BallGravity=-40\n", 5, 92},
                 BallCase {"BallCollisions=false\n", 5, 92}}) {
            QTemporaryDir dir;
            const auto path = dir.filePath(QStringLiteral("settingsrc"));
            QFile file(path);
            const auto input = QByteArray("[General]\n") + test.entry;
            QVERIFY(file.open(QIODevice::WriteOnly));
            file.write(input);
            file.close();
            Configuration config(path);
            QCOMPARE(config.ballCount(), test.count);
            QCOMPARE(config.ballElasticity(), test.elasticity);
            QVERIFY(file.open(QIODevice::ReadOnly));
            QCOMPARE(file.readAll(), input);
        }
    }

    void sharedEditsDuringDowngradeSurviveReupgrade()
    {
        const auto reset = qScopeGuard([] { VisualDepthRegistry::setAvailabilityForTest(QStringLiteral("kaleidoscope"), std::nullopt); });
        QTemporaryDir dir;
        const auto path = dir.filePath(QStringLiteral("settingsrc"));
        VisualDepthRegistry::setAvailabilityForTest(QStringLiteral("kaleidoscope"), true);
        {
            Configuration active(path);
            active.setVisualModule(QStringLiteral("kaleidoscope"));
            active.setAnimationSpeed(180);
            active.save();
        }
        {
            KConfig disk(path, KConfig::SimpleConfig);
            auto profile = KConfigGroup(&disk, QStringLiteral("Animations")).group(QStringLiteral("kaleidoscope"));
            profile.writeEntry("AnimationSpeed", 250);
            profile.writeEntry("AnimationDensity", 80);
            profile.writeEntry("AnimationScale", 135);
            profile.writeEntry("AnimationPalette", QStringLiteral("ember"));
            profile.writeEntry("TrailAmount", 70);
            QVERIFY(disk.sync());
        }
        Configuration upgraded(path);
        QCOMPARE(upgraded.animationSpeed(), 250);
        QCOMPARE(upgraded.animationDensity(), 80);
        QCOMPARE(upgraded.animationScale(), 135);
        QCOMPARE(upgraded.animationPalette(), QStringLiteral("ember"));
        QCOMPARE(upgraded.trailAmount(), 70);
    }
#endif

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

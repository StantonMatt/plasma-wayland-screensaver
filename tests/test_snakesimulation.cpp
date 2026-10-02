// SPDX-License-Identifier: GPL-3.0-or-later
#include "snakesimulation.h"
#include "snakerenderer.h"
#include "configuration.h"
#include <QTemporaryDir>
#include <QSignalSpy>
#include <QTest>
#include <limits>
#include <cmath>
#include <algorithm>

class SnakeSimulationTest final : public QObject
{
    Q_OBJECT
    static snakes_core_config defaults() { return {1280, 720, 50, 35, 100, 100, 75, 1, 6, 0, 1}; }
private Q_SLOTS:
    void configMapping()
    {
        QTemporaryDir dir;
        Configuration settings(dir.filePath(QStringLiteral("settingsrc")));
        settings.setAnimationDensity(90); settings.setTrailAmount(80);
        settings.setAnimationScale(150); settings.setAnimationSpeed(125);
        settings.setSnakeIntelligence(99); settings.setSnakeSelfCollisions(true);
        settings.setSnakeDeadlyWalls(false); settings.setAnimationPalette(QStringLiteral("ember"));
        const auto config = SnakeSimulation::configuration(settings, 1, 20000, 0xffffffffU);
        QCOMPARE(config.width, 80); QCOMPARE(config.height, 16384);
        QCOMPARE(config.density, 90); QCOMPARE(config.trails, 80);
        QCOMPARE(config.scale, 150); QCOMPARE(config.speed, 125);
        QCOMPARE(config.intelligence, 99); QCOMPARE(config.seed, -1);
        QCOMPARE(config.palette_size, 6U); QCOMPARE(config.self_collisions, 1U);
        QCOMPARE(config.deadly_walls, 0U);
        SnakeSimulation sim(config);
        sim.applySettings(settings);
        QCOMPARE(sim.palette().first(), QColor("#fff1a8"));
    }
    void fixedStepAccumulatorClampAndPause()
    {
        SnakeSimulation sim(defaults());
        QVERIFY(sim.isValid());
        sim.advance(1.0 / 60);
        QCOMPARE(sim.frame().info.tick, 0U);
        QVERIFY(std::abs(sim.interpolation() - 0.5) < 1e-9);
        sim.advance(1.0 / 60);
        QCOMPARE(sim.frame().info.tick, 1U);
        QCOMPARE(sim.interpolation(), 0);
        sim.advance(4);
        QCOMPARE(sim.frame().info.tick, 4U);
        sim.advance(-1); sim.advance(std::numeric_limits<double>::quiet_NaN());
        QCOMPARE(sim.frame().info.tick, 4U);
        sim.setPaused(true); sim.advance(1);
        QCOMPARE(sim.frame().info.tick, 4U);
        sim.setPaused(false); sim.advance(1.0 / 30);
        QCOMPARE(sim.frame().info.tick, 5U);
    }
    void logicalExtentSurvivesAbiLimitAndSettings_data()
    {
        QTest::addColumn<QSizeF>("extent");
        QTest::newRow("wide") << QSizeF(24000, 1200);
        QTest::newRow("tall") << QSizeF(1600, 24000);
        QTest::newRow("both") << QSizeF(24000, 20000);
        QTest::newRow("small-viewport") << QSizeF(60, 40);
    }
    void logicalExtentSurvivesAbiLimitAndSettings()
    {
        QFETCH(QSizeF, extent);
        QTemporaryDir dir;
        Configuration settings(dir.filePath(QStringLiteral("settingsrc")));
        SnakeSimulation sim(SnakeSimulation::configuration(settings, extent.width(), extent.height(), 1));
        QVERIFY(sim.resize(extent.width(), extent.height()));
        QCOMPARE(sim.viewSize(), extent);
        QCOMPARE(sim.config().width, std::clamp(extent.width(), 80.0, 16384.0));
        QCOMPARE(sim.config().height, std::clamp(extent.height(), 80.0, 16384.0));
        sim.advance(1.0 / 30);
        settings.setAnimationSpeed(125);
        settings.setAnimationPalette(QStringLiteral("ember"));
        sim.applySettings(settings);
        QCOMPARE(sim.viewSize(), extent);
        QCOMPARE(sim.frame().info.tick, 1U);
        QSignalSpy presented(&sim, &SnakeSimulation::presented);
        const auto nextExtent = extent * 1.1;
        QVERIFY(sim.resize(nextExtent.width(), nextExtent.height()));
        QCOMPARE(sim.viewSize(), nextExtent);
        QCOMPARE(presented.count(), 1);
        QCOMPARE(sim.frame().info.tick, 1U);
        QVERIFY(!sim.resize(std::numeric_limits<double>::infinity(), 100));
        QCOMPARE(sim.viewSize(), nextExtent);
    }
    void resizeAndReconfigurePreserveState()
    {
        SnakeSimulation sim(defaults());
        sim.advance(0.1);
        const auto info = sim.frame().info;
        const auto head = sim.frame().segments[0];
        QVERIFY(sim.resize(2560, 1440));
        QCOMPARE(sim.frame().info.tick, info.tick);
        QCOMPARE(sim.frame().info.simulation_time, info.simulation_time);
        QCOMPARE(sim.frame().segments[0].x, head.x * 2);
        QCOMPARE(sim.frame().segments[0].previous_x, head.previous_x * 2);
        QVERIFY(sim.frame().info.geometry_generation > info.geometry_generation);
        auto config = sim.config(); config.speed = 130; config.intelligence = 100;
        QVERIFY(sim.reconfigure(config));
        QCOMPARE(sim.frame().info.tick, info.tick);
        config.seed = 2;
        QVERIFY(sim.reconfigure(config));
        QCOMPARE(sim.frame().info.tick, 0U);
        sim.advance(1.0 / 60);
        config.density = 100;
        QVERIFY(sim.reconfigure(config));
        QCOMPARE(sim.frame().info.tick, 0U);
        QCOMPARE(sim.interpolation(), 0);
        const auto width = sim.config().width;
        config.width = -1;
        QVERIFY(!sim.reconfigure(config));
        QCOMPARE(sim.config().width, width);
    }
    void sharedPresentationDoesNotMultiplyStepsAndSurvivesViewRemoval()
    {
        SnakeSimulation sim(defaults());
        auto *firstView = new QQuickItem;
        auto *first = new SnakeRenderer(firstView);
        SnakeRenderer second;
        first->setSimulation(&sim); second.setSimulation(&sim);
        // One logical clock advances the world; both views receive the frame.
        QSignalSpy presented(&sim, &SnakeSimulation::presented);
        sim.advance(1.0 / 30);
        QCOMPARE(sim.frame().info.tick, 1U);
        QCOMPARE(presented.count(), 1);
        delete firstView;
        QVERIFY(second.simulation() == &sim);
        sim.advance(1.0 / 30);
        QCOMPARE(sim.frame().info.tick, 2U);
    }
    void exportBuffersRetainHighWaterCapacity()
    {
        SnakeSimulation sim(defaults());
        // A mature high-water allocation remains reusable after a restart.
        for (int i = 0; i < 3000; ++i) sim.advance(1.0 / 30);
        const auto snakesCapacity = sim.frame().snakes.capacity();
        const auto segmentsCapacity = sim.frame().segments.capacity();
        const auto foodCapacity = sim.frame().food.capacity();
        auto config = sim.config(); config.seed = 3;
        QVERIFY(sim.reconfigure(config));
        const auto *snakes = sim.frame().snakes.data();
        const auto *segments = sim.frame().segments.data();
        const auto *food = sim.frame().food.data();
        for (int i = 0; i < 30; ++i) sim.advance(1.0 / 60);
        QCOMPARE(sim.frame().snakes.capacity(), snakesCapacity);
        QCOMPARE(sim.frame().segments.capacity(), segmentsCapacity);
        QCOMPARE(sim.frame().food.capacity(), foodCapacity);
        QCOMPARE(sim.frame().snakes.data(), snakes);
        QCOMPARE(sim.frame().segments.data(), segments);
        QCOMPARE(sim.frame().food.data(), food);
    }
};
QTEST_MAIN(SnakeSimulationTest)
#include "test_snakesimulation.moc"

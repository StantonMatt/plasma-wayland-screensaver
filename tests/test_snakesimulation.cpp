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
#include <array>
#include <vector>

class SnakeSimulationTest final : public QObject
{
    Q_OBJECT
    static snakes_core_config defaults() { return {1280, 720, 50, 35, 100, 100, 75, 1, 6, 0, 1, SNAKES_CORE_RULE_DEFAULT, 0}; }
private Q_SLOTS:
    void compactHistoryOutlivesPhaseHistoryAndResetsOnRestart()
    {
        SnakeSimulation sim(defaults());
        sim.setPresentationLead(4'166'667);
        qint64 time = 1'000'000'000;
        sim.advanceTo(time);
        for (int i = 0; i < 100; ++i) sim.advanceTo(time += 33'333'334);
        QVERIFY(sim.m_historyCount < SnakeSimulation::maximumHistoryFrames);
        std::array<SnakePresentationFrame, SnakeSimulation::maximumHistoryFrames> history;
        size_t head = 0, count = 0;
        sim.retainPresentationHistory(nullptr, sim.frame().info, history, head, count);
        QCOMPARE(count, SnakeSimulation::maximumHistoryFrames);
        for (size_t i = 0; i < count; ++i) {
            const auto &state = history[(head + history.size() - count + i) % history.size()];
            QCOMPARE(state.info.tick, sim.frame().info.tick - count + i + 1);
            for (size_t e = 0; e < state.eventCount; ++e)
                QCOMPARE(state.events[e].tick, state.info.tick);
        }
        const auto &last = history[(head + history.size() - 1) % history.size()];
        QCOMPARE(last.snakeCount, sim.frame().snakes.size());
        QCOMPARE(last.foodCount, sim.frame().food.size());
        for (size_t i = 0; i < last.snakeCount; ++i) {
            const auto &snake = sim.frame().snakes[i];
            QCOMPARE(last.snakes[i].flags, snake.flags);
            QCOMPARE(last.snakes[i].generation, snake.generation);
            if (snake.segment_count) {
                const auto &tail = sim.frame().segments[snake.segment_offset + snake.segment_count - 1];
                QCOMPARE(last.tails[i].x, tail.x); QCOMPARE(last.tails[i].y, tail.y);
            }
        }
        QVERIFY(sim.resize(2560, 1440));
        head = count = 0;
        sim.retainPresentationHistory(nullptr, sim.frame().info, history, head, count);
        QCOMPARE(count, size_t(1));
        QCOMPARE(history[0].info.geometry_generation, sim.frame().info.geometry_generation);
        auto config = sim.config(); ++config.seed;
        QVERIFY(sim.reconfigure(config));
        head = count = 0;
        sim.retainPresentationHistory(nullptr, sim.frame().info, history, head, count);
        QCOMPARE(count, size_t(1)); QCOMPARE(history[0].info.tick, uint64_t(0));
    }

    void mixedRefreshWindowsShareAbsoluteTimeline_data()
    {
        QTest::addColumn<double>("phase");
        QTest::newRow("aligned-slow-output") << 0.0;
        QTest::newRow("eighth-refresh-phase") << 0.125;
        QTest::newRow("half-refresh-phase") << 0.5;
        QTest::newRow("late-refresh-phase") << 0.875;
    }
    void mixedRefreshWindowsShareAbsoluteTimeline()
    {
        QFETCH(double, phase);
        struct Request { qint64 wake, presentation; int window; };
        constexpr qint64 origin = 987'654'321'000'000;
        const double refresh[] = {24, 60, 100, 144, 175, 240};
        const int divisor[] = {1, 1, 2, 3, 3, 4};
        std::vector<Request> requests;
        for (int window = 0; window < 6; ++window) {
            const double refreshPeriod = 1e9 / refresh[window];
            for (int frame = 1; frame <= 500; ++frame) {
                const qint64 presentation = origin + std::llround((frame * divisor[window] + window * 0.37 + phase * window * 0.173) * refreshPeriod);
                requests.push_back({presentation - std::llround(refreshPeriod), presentation, window});
            }
        }
        std::sort(requests.begin(), requests.end(), [](const auto &a, const auto &b) { return a.wake < b.wake; });
        SnakeSimulation sim(defaults());
        sim.setPresentationLead(41'666'667);
        sim.advanceTo(origin);
        std::array<SnakeRenderer, 6> views;
        for (auto &view : views) view.setSimulation(&sim);
        QSignalSpy fanout(&sim, &SnakeSimulation::presented);
        qint64 highWater = origin;
        int backwardsRequests = 0, historyReads = 0;
        for (const auto &request : requests) {
            const auto previousTick = sim.frame().info.tick;
            sim.advanceTo(request.presentation);
            if (request.presentation < highWater) {
                ++backwardsRequests;
                QCOMPARE(sim.frame().info.tick, previousTick);
            }
            highWater = std::max(highWater, request.presentation);
            views[request.window].presentAt(request.presentation);
            double alpha = 0;
            const auto &frame = sim.frameAt(request.presentation, alpha);
            if (frame.info.tick < sim.frame().info.tick) ++historyReads;
            const double time = (request.presentation - origin) / 1e9;
            QVERIFY(std::abs(frame.info.simulation_time + alpha * sim.physicsStepSeconds() - time) < 1e-8);
            QCOMPARE(sim.frame().info.tick, uint64_t(std::floor((highWater - origin) / 1e9 * 30 + 1e-10)));
        }
        QVERIFY(backwardsRequests > 0);
        QVERIFY(historyReads > 0);
        QCOMPARE(fanout.count(), 0); // Other windows must not submit on this tick.
        const auto tick = sim.frame().info.tick;
        sim.advanceTo(highWater); sim.advanceTo(origin);
        QCOMPARE(sim.frame().info.tick, tick);
        // Removing a view or changing its refresh does not reset shared time.
        views[0].setSimulation(nullptr);
        sim.advanceTo(highWater + 40'000'000);
        QCOMPARE(sim.frame().info.tick, uint64_t(std::floor((highWater + 40'000'000 - origin) / 1e9 * 30)));
    }

    void absoluteTimelineStallPauseAndStorageReuse()
    {
        SnakeSimulation sim(defaults());
        constexpr qint64 origin = 1'000'000'000;
        sim.advanceTo(origin);
        sim.advanceTo(origin + 2'000'000'000);
        QCOMPARE(sim.frame().info.tick, 3U); // One clamp, regardless of view count.
        double alpha = 0;
        const auto &previous = sim.frameAt(origin + 1'990'000'000, alpha);
        QCOMPARE(previous.info.tick, 2U);
        QVERIFY(std::abs(alpha - 0.7) < 1e-9);
        sim.setPaused(true);
        sim.advanceTo(origin + 3'000'000'000);
        sim.advanceTo(origin + 4'000'000'000);
        QCOMPARE(sim.frame().info.tick, 3U);
        sim.setPaused(false);
        sim.advanceTo(origin + 5'000'000'000);
        sim.advanceTo(origin + 5'033'333'334);
        QCOMPARE(sim.frame().info.tick, 4U);
        // Warm every ring export buffer to the mature world's high water.
        qint64 time = origin + 5'033'333'334;
        for (int i = 0; i < 3000; ++i) sim.advanceTo(time += 33'333'334);
        std::array<const snakes_core_segment *, SnakeSimulation::maximumHistoryFrames> segments;
        std::array<size_t, SnakeSimulation::maximumHistoryFrames> capacity;
        for (int i = 0; size_t(i) < segments.size(); ++i) {
            sim.advanceTo(time += 33'333'334);
            segments[i] = sim.frame().segments.data();
            capacity[i] = sim.frame().segments.capacity();
        }
        for (int i = 0; i < 100; ++i) {
            sim.advanceTo(time += 33'333'334);
            QCOMPARE(sim.frame().segments.data(), segments[i % segments.size()]);
            QCOMPARE(sim.frame().segments.capacity(), capacity[i % capacity.size()]);
        }
    }

    void leasedHistoryStorageStaysBoundedAndReusesEveryBuffer()
    {
        SnakeSimulation sim(defaults());
        sim.setPresentationLead(41'666'667); // Slowest supported output in this fixture: 24 Hz.
        SnakeRenderer delayed, current;
        delayed.setSimulation(&sim); current.setSimulation(&sim);
        qint64 time = 1'000'000'000;
        sim.advanceTo(time);
        for (int i = 0; i < 4000; ++i) {
            sim.advanceTo(time += 33'333'334);
            delayed.presentAt(time); current.presentAt(time);
        }
        // Leave one renderer's request pending while the other wraps the ring.
        for (int i = 0; i < 10; ++i) {
            sim.advanceTo(time += 33'333'334);
            current.presentAt(time);
        }
        QCOMPARE(sim.m_historyLimit, size_t(4));
        QVERIFY(sim.m_storage.size() <= 6); // Four history frames + two viewport leases.
        const size_t poolSize = sim.m_storage.size();
        struct Storage { const void *snakes, *segments, *food; size_t a, b, c; };
        std::vector<Storage> buffers;
        for (const auto &frame : sim.m_storage)
            buffers.push_back({frame->snakes.data(), frame->segments.data(), frame->food.data(),
                               frame->snakes.capacity(), frame->segments.capacity(), frame->food.capacity()});
        for (int i = 0; i < 200; ++i) {
            sim.advanceTo(time += 33'333'334);
            current.presentAt(time);
        }
        QCOMPARE(sim.m_storage.size(), poolSize); // No new frame/control-block allocations.
        for (size_t i = 0; i < poolSize; ++i) {
            const auto &frame = sim.m_storage[i];
            QCOMPARE(frame->snakes.data(), buffers[i].snakes);
            QCOMPARE(frame->segments.data(), buffers[i].segments);
            QCOMPARE(frame->food.data(), buffers[i].food);
            QCOMPARE(frame->snakes.capacity(), buffers[i].a);
            QCOMPARE(frame->segments.capacity(), buffers[i].b);
            QCOMPARE(frame->food.capacity(), buffers[i].c);
        }
        const auto newest = sim.frame().info.tick;
        sim.setPresentationLead(4'166'667); // Refresh change to 240 Hz.
        sim.advanceTo(time + 33'333'334);
        QCOMPARE(sim.frame().info.tick, newest + 1);
        delayed.setSimulation(nullptr); // Removal releases the pending lease.
        sim.setPaused(true); sim.setPaused(false);
        double alpha = 0;
        QCOMPARE(sim.frameAt(0, alpha).info.tick, sim.frame().info.tick);
        QCOMPARE(alpha, 0.0); // Old history cannot leak through pause/resume.
    }

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
        for (int i = 0; i < 100; ++i) sim.advance(1.0 / 60);
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

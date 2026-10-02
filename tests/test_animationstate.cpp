// SPDX-License-Identifier: GPL-3.0-or-later
#include "../src/animationstate.h"

#include <QTest>
#include <QSignalSpy>
#include <QVariantMap>
#include <QLineF>
#include <algorithm>
#include <cmath>
#include <vector>

class AnimationStateTest final : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void physicsDoesNotDependOnPresentationCadence()
    {
        AnimationState sixty, fortyEight;
        for (auto *state : {&sixty, &fortyEight})
            state->configureGeometries({QRect(0, 0, 640, 480)}, true, true,
                                      QStringLiteral("fast"), 60, 20, 300, 100,
                                      100, 65, true, true);
        for (int second = 0; second < 5; ++second) {
            for (int i = 0; i < 60; ++i) sixty.advance(1.0 / 60);
            for (int i = 0; i < 48; ++i) fortyEight.advance(1.0 / 48);
            QCOMPARE(sixty.balls(), fortyEight.balls());
            QCOMPARE(sixty.clockX(), fortyEight.clockX());
            QCOMPARE(sixty.clockY(), fortyEight.clockY());
        }
    }

    void sharedTimelineSamplesEveryDisplayPhase()
    {
        struct Request { qint64 wake, presentation; QVariantList balls; QPointF clock; };
        constexpr qint64 origin = 987'654'321'000'000;
        constexpr double refresh[] = {24, 60, 100, 144, 175, 240};
        constexpr int divisor[] = {1, 1, 2, 3, 3, 4};
        std::vector<Request> requests;
        for (int window = 0; window < 6; ++window) {
            const double period = 1e9 / refresh[window];
            for (int frame = 1; frame <= 150; ++frame) {
                const qint64 time = origin + std::llround((frame * divisor[window] + window * 0.37) * period);
                requests.push_back({time - std::llround(period), time, {}, {}});
            }
        }
        AnimationState chronological, shared;
        for (auto *state : {&chronological, &shared}) {
            state->configureGeometries({QRect(0, 0, 640, 480)}, true, true,
                                      QStringLiteral("fast"), 60, 12, 300, 100, 100, 65, true, true);
            state->setClockSize(100, 60);
            state->advanceTo(origin);
        }
        // A chronologically driven world supplies reference samples; a world
        // driven in GUI wake order must return the same collision trajectories.
        std::sort(requests.begin(), requests.end(), [](auto &a, auto &b) { return a.presentation < b.presentation; });
        for (auto &request : requests) {
            chronological.advanceTo(request.presentation);
            request.balls = chronological.ballsAt(request.presentation);
            request.clock = chronological.clockAt(request.presentation);
        }
        std::sort(requests.begin(), requests.end(), [](auto &a, auto &b) { return a.wake < b.wake; });
        QSignalSpy fanout(&shared, &AnimationState::frameChanged);
        for (const auto &request : requests) {
            shared.advanceTo(request.presentation);
            const auto balls = shared.ballsAt(request.presentation);
            for (int i = 0; i < balls.size(); ++i) {
                const auto actual = balls[i].toMap(), expected = request.balls[i].toMap();
                for (const auto &key : {QStringLiteral("x"), QStringLiteral("y"), QStringLiteral("vx"), QStringLiteral("vy")})
                    QVERIFY(std::abs(actual[key].toReal() - expected[key].toReal()) < 1e-8);
            }
            QVERIFY(QLineF(shared.clockAt(request.presentation), request.clock).length() < 1e-8);
        }
        QCOMPARE(fanout.count(), 0);
    }

    void refreshReconfigurePreservesBallsAndHistoryPauseDoesNotCatchUp()
    {
        AnimationState state;
        const auto configure = [&](int rate, bool motion) {
            state.configureGeometries({QRect(0, 0, 640, 480)}, motion, motion,
                                      QStringLiteral("normal"), rate, 3, 100, 100, 0, 100, false, true);
        };
        configure(60, true);
        state.setClockSize(100, 60);
        constexpr qint64 origin = 1'000'000'000;
        state.advanceTo(origin); state.advanceTo(origin + 100'000'000);
        const auto oldSample = state.ballsAt(origin + 50'000'000);
        const auto balls = state.balls();
        const auto clock = state.clockAt(origin + 100'000'000);
        configure(48, true);
        QCOMPARE(state.balls(), balls);
        QCOMPARE(state.ballsAt(origin + 50'000'000), oldSample);
        QCOMPARE(state.clockAt(origin + 100'000'000), clock);
        configure(48, false); configure(48, true);
        const auto resumed = state.balls();
        state.advanceTo(origin + 10'000'000'000);
        QCOMPARE(state.balls(), resumed); // Resume establishes a fresh time anchor.
        state.advanceTo(origin + 10'020'000'000);
        QVERIFY(state.balls() != resumed);
    }

    void respectsSteppedMonitorUnion()
    {
        AnimationState state;
        state.configureGeometries({QRect(0, 0, 3440, 1440), QRect(3440, 166, 1920, 1080)},
                                  false, false, QStringLiteral("normal"), 30);
        const qreal size = state.ballSize();

        QVERIFY(state.containsRect(QRectF(100, 100, size, size)));
        QVERIFY(state.containsRect(QRectF(3500, 200, size, size)));
        QVERIFY(!state.containsRect(QRectF(3500, 20, size, size)));

        // A ball may straddle the connected seam only where both monitors
        // contribute pixels to the union at that vertical position.
        QVERIFY(state.containsRect(QRectF(3400, 200, size, size)));
        QVERIFY(!state.containsRect(QRectF(3400, 80, size, size)));

        const qreal lowMonitorBottom = 166 + 1080;
        QVERIFY(state.containsRect(QRectF(3500, lowMonitorBottom - size, size, size)));
        QVERIFY(!state.containsRect(QRectF(3500, lowMonitorBottom - size + 2, size, size)));
    }

    void buildsConfiguredBallSet()
    {
        AnimationState state;
        state.configureGeometries({QRect(0, 0, 1920, 1080)}, false, false,
                                  QStringLiteral("normal"), 30, 12, 180, 140,
                                  -35, 78, true);
        QCOMPARE(state.balls().size(), 12);
        QVERIFY(state.ballSize() > 80);
        for (const QVariant &entry : state.balls()) {
            const QVariantMap ball = entry.toMap();
            QVERIFY(state.containsRect(QRectF(ball.value(QStringLiteral("x")).toReal(),
                                              ball.value(QStringLiteral("y")).toReal(),
                                              ball.value(QStringLiteral("size")).toReal(),
                                              ball.value(QStringLiteral("size")).toReal())));
        }
    }

    void activePhysicsStaysInsideSteppedDesktop()
    {
        AnimationState state;
        state.configureGeometries({QRect(0, 0, 3440, 1440), QRect(3440, 166, 1920, 1080)},
                                  true, false, QStringLiteral("normal"), 60,
                                  20, 300, 160, 100, 65, true);
        QSignalSpy frames(&state, &AnimationState::frameChanged);
        QTRY_VERIFY(frames.count() >= 15);
        QCOMPARE(state.balls().size(), 20);
        for (const QVariant &entry : state.balls()) {
            const QVariantMap ball = entry.toMap();
            const qreal size = ball.value(QStringLiteral("size")).toReal();
            QVERIFY(state.containsRect(QRectF(ball.value(QStringLiteral("x")).toReal(),
                                              ball.value(QStringLiteral("y")).toReal(),
                                              size, size)));
        }
    }

    void externalClockDrivesSeamlessPhysicsExclusively()
    {
        AnimationState state;
        state.configureGeometries({QRect(0, 0, 1920, 1080)}, true, false,
                                  QStringLiteral("normal"), 240,
                                  3, 100, 100, 0, 100, false, true);
        const QVariantMap before = state.balls().constFirst().toMap();
        QSignalSpy frames(&state, &AnimationState::frameChanged);
        QTest::qWait(30);
        QCOMPARE(frames.count(), 0);
        QCOMPARE(state.balls().constFirst().toMap().value(QStringLiteral("x")),
                 before.value(QStringLiteral("x")));

        for (int i = 0; i < 4; ++i) state.advance(1.0 / 240.0);
        QCOMPARE(frames.count(), 4);
        QVERIFY(state.balls().constFirst().toMap().value(QStringLiteral("x")).toReal()
                != before.value(QStringLiteral("x")).toReal());
    }
};

QTEST_GUILESS_MAIN(AnimationStateTest)
#include "test_animationstate.moc"

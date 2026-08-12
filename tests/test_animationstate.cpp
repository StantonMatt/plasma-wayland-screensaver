// SPDX-License-Identifier: GPL-3.0-or-later
#include "../src/animationstate.h"

#include <QTest>
#include <QSignalSpy>
#include <QVariantMap>

class AnimationStateTest final : public QObject
{
    Q_OBJECT

private Q_SLOTS:
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
        QTest::qWait(250);
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

        state.advance(1.0 / 240.0);
        QCOMPARE(frames.count(), 1);
        QVERIFY(state.balls().constFirst().toMap().value(QStringLiteral("x")).toReal()
                != before.value(QStringLiteral("x")).toReal());
    }
};

QTEST_GUILESS_MAIN(AnimationStateTest)
#include "test_animationstate.moc"

// SPDX-License-Identifier: GPL-3.0-or-later
#include "../src/animationstate.h"

#include <QTest>

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
};

QTEST_GUILESS_MAIN(AnimationStateTest)
#include "test_animationstate.moc"

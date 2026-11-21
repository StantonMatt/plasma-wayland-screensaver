// SPDX-License-Identifier: GPL-3.0-or-later
#include "visuals/kaleidoscope/blur.h"
#include <QtTest>
#include <algorithm>
#include <cmath>

class BlurTest : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void liveInputsUpdateWithoutResize()
    {
        KaleidoscopeBlurUniforms state;
        kaleidoscope_lens lens {};
        lens.eye[2] = 676.8f;
        lens.glass[1] = 1.f;
        const auto target = [&] {
            const int width = std::clamp(qRound(1.55 * lens.eye[2] * 2 * std::sqrt(lens.glass[1])), 256, 1536);
            return QSize(width, qRound(width * 0.6));
        };
        const QSize size = target();
        QCOMPARE(size.width(), 1536);
        QVERIFY(state.sync(lens));
        const auto initial = state.take(size, -1.f);
        QVERIFY(initial.has_value());
        QVERIFY(!state.sync(lens));
        QVERIFY(!state.take(size, -1.f));

        lens.glass[0] = 1.f; // Glow 0 -> 100: seven times the spread.
        QCOMPARE(target(), size);
        QVERIFY(state.sync(lens));
        const auto glow = state.take(size, -1.f);
        QVERIFY(glow.has_value());
        for (size_t i = 0; i < 2; ++i) {
            for (size_t axis = 0; axis < 2; ++axis)
                QVERIFY(std::abs((*glow)[i][axis] / (*initial)[i][axis] - 7.f) < 1e-5f);
            QCOMPARE((*glow)[i][2], -1.f);
        }
        lens.glass[1] = 2.f; // Zoom changes while the target remains clamped.
        QCOMPARE(target(), size);
        QVERIFY(state.sync(lens));
        const auto zoom = state.take(size, -1.f);
        QVERIFY(zoom.has_value());
        for (size_t i = 0; i < 2; ++i)
            for (size_t axis = 0; axis < 2; ++axis)
                QCOMPARE((*zoom)[i][axis], (*glow)[i][axis] * 0.5f);

        lens.eye[2] *= 2.f; // A different host eye can also keep the same target.
        QCOMPARE(target(), size);
        QVERIFY(state.sync(lens));
        const auto eye = state.take(size, -1.f);
        QVERIFY(eye.has_value());
        for (size_t i = 0; i < 2; ++i)
            for (size_t axis = 0; axis < 2; ++axis)
                QCOMPARE((*eye)[i][axis], (*zoom)[i][axis] * 0.5f);
        QVERIFY(!state.sync(lens));
        state.invalidate(); // Resource recreation still uploads unchanged inputs.
        QVERIFY(state.sync(lens));
        QCOMPARE(state.take(size, -1.f), eye);
    }
};
QTEST_GUILESS_MAIN(BlurTest)
#include "test_blur.moc"

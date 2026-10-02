// SPDX-License-Identifier: GPL-3.0-or-later
#include "presentationclock.h"

#include <QElapsedTimer>
#include <QEventLoop>
#include <QQuickWindow>
#include <QSignalSpy>
#include <QScreen>
#include <QTest>
#include <QTimer>

#include <algorithm>
#include <cmath>
#include <limits>
#include <vector>

using PresentationPacing::periodNanoseconds;
using PresentationPacing::schedule;

class PresentationClockTest final : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void divisorSelection_data()
    {
        QTest::addColumn<qreal>("refresh");
        QTest::addColumn<int>("target");
        QTest::addColumn<int>("divisor");
        const qreal refreshes[] = {99.946, 174.962, 239.761, 59.94, 144, 165, 120, 75};
        const int at60[] = {2, 3, 4, 1, 3, 3, 2, 2};
        const int at30[] = {4, 6, 8, 2, 5, 6, 4, 3};
        for (int i = 0; i < 8; ++i) {
            for (int target : {0, 30, 60}) {
                const auto name = QStringLiteral("%1-at-%2").arg(refreshes[i]).arg(target).toLatin1();
                QTest::newRow(name.constData()) << refreshes[i] << target
                    << (target == 0 ? 1 : target == 30 ? at30[i] : at60[i]);
            }
        }
    }

    void divisorSelection()
    {
        QFETCH(qreal, refresh);
        QFETCH(int, target);
        QFETCH(int, divisor);
        QCOMPARE(PresentationPacing::refreshDivisor(target, refresh), divisor);
        const long double period = periodNanoseconds(target, refresh);
        QVERIFY(std::abs(period - divisor * 1e9L / refresh) < 1e-6L);
        if (target > 0) {
            QVERIFY(refresh / divisor <= target);
            QVERIFY(divisor == 1 || refresh / (divisor - 1) > target);
        }
        constexpr qint64 origin = 987'654'321'000'000;
        long double ideal = origin + period;
        qint64 previous = origin;
        for (int i = 0; i < 3000; ++i) {
            const auto next = schedule(previous + 200'000, ideal, target, refresh, origin);
            QVERIFY(std::abs(next.presentationNanoseconds - previous - period) < 2);
            previous = next.presentationNanoseconds;
            ideal = next.nextTargetNanoseconds;
        }
    }
    void steadyCadence_data()
    {
        QTest::addColumn<qreal>("refresh");
        QTest::addColumn<int>("target");
        QTest::addColumn<int>("minimumSlots");
        QTest::addColumn<int>("maximumSlots");
        QTest::newRow("60-auto") << 60.0 << 0 << 1 << 1;
        QTest::newRow("60-60") << 60.0 << 60 << 1 << 1;
        QTest::newRow("60-30") << 60.0 << 30 << 2 << 2;
        QTest::newRow("144-auto") << 144.0 << 0 << 1 << 1;
        QTest::newRow("144-144") << 144.0 << 144 << 1 << 1;
        QTest::newRow("144-240-capped") << 144.0 << 240 << 1 << 1;
        QTest::newRow("144-60") << 144.0 << 60 << 3 << 3;
        QTest::newRow("144-30") << 144.0 << 30 << 5 << 5;
        QTest::newRow("75-30") << 75.0 << 30 << 3 << 3;
        QTest::newRow("75-60") << 75.0 << 60 << 2 << 2;
        QTest::newRow("75-auto") << 75.0 << 0 << 1 << 1;
        QTest::newRow("59.94-30") << 59.94 << 30 << 2 << 2;
    }

    void steadyCadence()
    {
        QFETCH(qreal, refresh);
        QFETCH(int, target);
        QFETCH(int, minimumSlots);
        QFETCH(int, maximumSlots);

        // Use realistic monotonic uptime, not just times near zero. Simulated
        // GUI callback latency and render work must not move the ideal grid.
        constexpr qint64 origin = 987'654'321'000'000;
        const long double period = periodNanoseconds(target, refresh);
        const long double refreshPeriod = 1'000'000'000.0L / refresh;
        long double ideal = origin + period;
        qint64 swap = origin;
        constexpr int frames = 3000;
        qint64 totalSlots = 0;
        for (int frame = 1; frame <= frames; ++frame) {
            const qint64 now = swap + 200'000;
            const auto next = schedule(now, ideal, target, refresh, origin);
            const qint64 slots = std::llround((next.presentationNanoseconds - swap) / refreshPeriod);
            QVERIFY(slots >= minimumSlots);
            QVERIFY(slots <= maximumSlots);
            totalSlots += slots;
            QVERIFY(next.wakeNanoseconds >= now);
            QVERIFY(next.wakeNanoseconds < next.presentationNanoseconds);
            QVERIFY(std::abs(next.presentationNanoseconds - ideal) <= refreshPeriod / 2 + 2);
            QVERIFY(std::abs(next.nextTargetNanoseconds - (origin + (frame + 1) * period)) < 1);
            // A swap-anchored wake gives a whole refresh for work, except at
            // full rate where callback delivery has already used 200 us.
            QVERIFY(next.presentationNanoseconds - next.wakeNanoseconds >= refreshPeriod - 200'001);
            ideal = next.nextTargetNanoseconds;
            swap = std::llround(origin + totalSlots * refreshPeriod);
        }
        const qint64 expectedSlots = std::llround(frames * period / refreshPeriod);
        QCOMPARE(totalSlots, expectedSlots);
    }

    void oddRefreshUsesEvenDivisor()
    {
        constexpr qint64 origin = 1'000'000'000;
        const long double period = periodNanoseconds(30, 75);
        constexpr long double refreshPeriod = 1'000'000'000.0L / 75;
        qint64 swap = origin;
        long double ideal = origin + period;
        qint64 previousSlot = 0;
        for (int frame = 1; frame <= 20; ++frame) {
            const auto next = schedule(swap, ideal, 30, 75, origin);
            const qint64 slot = std::llround((next.presentationNanoseconds - origin) / refreshPeriod);
            const qint64 nearestSlot = std::llround(frame * 3.0L);
            QCOMPARE(slot, nearestSlot);
            QCOMPARE(slot - previousSlot, 3LL);
            previousSlot = slot;
            ideal = next.nextTargetNanoseconds;
            swap = std::llround(origin + nearestSlot * refreshPeriod);
        }
    }

    void lateSwapSkipsMissedSlots()
    {
        constexpr qint64 origin = 1'000'000'000;
        const long double period = periodNanoseconds(30, 60);
        const qint64 lateSwap = origin + 500'000'000;
        const auto next = schedule(lateSwap + 200'000, origin + 2 * period,
                                   30, 60, lateSwap);
        QVERIFY(std::abs(next.presentationNanoseconds - (origin + 16 * period)) < 1);
        QVERIFY(std::abs(next.nextTargetNanoseconds - (origin + 17 * period)) < 1);
        // One future frame, followed by a fresh 33.3 ms target; no catch-up burst.
        const auto following = schedule(next.presentationNanoseconds + 200'000,
                                        next.nextTargetNanoseconds, 30, 60,
                                        next.presentationNanoseconds);
        QVERIFY(std::abs((following.presentationNanoseconds - next.presentationNanoseconds) - period) < 2);
    }

    void queuedCallbackStallSkipsPastVsyncs()
    {
        constexpr qint64 origin = 1'000'000'000;
        const long double period = periodNanoseconds(30, 60);
        const qint64 now = origin + 421'000'000;
        const auto next = schedule(now, origin + period, 30, 60, origin);
        QVERIFY(std::abs(next.presentationNanoseconds - (origin + 13 * period)) < 1);
        QCOMPARE(next.wakeNanoseconds, now);
        QVERIFY(next.nextTargetNanoseconds > next.presentationNanoseconds);

        // Even a multi-hour stall is handled arithmetically in one decision.
        const qint64 hoursLater = origin + 8 * 3600LL * 1'000'000'000 + 200'000;
        const auto afterHours = schedule(hoursLater, origin + period, 30, 60, origin);
        QVERIFY(afterHours.presentationNanoseconds > hoursLater);
        QVERIFY(afterHours.presentationNanoseconds - hoursLater < period + 1);
        QVERIFY(afterHours.nextTargetNanoseconds > afterHours.presentationNanoseconds);
    }

    void swapJitterDoesNotAccumulate()
    {
        constexpr qint64 origin = 987'654'321'000'000;
        const long double period = periodNanoseconds(30, 60);
        constexpr qint64 jitter[] = {100'000, -90'000, 20'000, -30'000, 0};
        long double ideal = origin + period;
        qint64 swap = origin;
        for (int frame = 1; frame <= 10000; ++frame) {
            const auto next = schedule(swap + 250'000, ideal, 30, 60, swap);
            QCOMPARE(std::llround((next.presentationNanoseconds - swap) / period), 1LL);
            QVERIFY(std::abs(next.presentationNanoseconds - (origin + frame * period)) < 101'000);
            QVERIFY(std::abs(next.nextTargetNanoseconds - (origin + (frame + 1) * period)) < 2);
            ideal = next.nextTargetNanoseconds;
            // Jitter around the actual output grid, rather than accumulating
            // artificially injected error from the last prediction.
            swap = std::llround(origin + frame * period) + jitter[frame % 5];
        }
    }

    void timerDeadlinesStayAbsolute()
    {
        constexpr qint64 origin = 987'654'321'000'000;
        const long double period = periodNanoseconds(30, 60);
        long double ideal = origin + period;
        for (int frame = 1; frame <= 3000; ++frame) {
            // The event loop delivers each timer 700 us late. A late wake must
            // still issue this frame, and must not delay the next deadline.
            const qint64 now = std::llround(origin + frame * period) + 700'000;
            const auto next = schedule(now, ideal, 30, 60);
            QCOMPARE(next.wakeNanoseconds, now);
            QCOMPARE(next.presentationNanoseconds, now);
            QVERIFY(std::abs(next.nextTargetNanoseconds - (origin + (frame + 1) * period)) < 1);
            ideal = next.nextTargetNanoseconds;
        }
        const auto stall = schedule(origin + 501'000'000, origin + period, 30, 60);
        QCOMPARE(stall.wakeNanoseconds, origin + 501'000'000);
        QVERIFY(std::abs(stall.nextTargetNanoseconds - (origin + 16 * period)) < 1);
        QVERIFY(stall.nextTargetNanoseconds > stall.wakeNanoseconds);
    }

    void submissionLatencyDoesNotMovePhase_data()
    {
        QTest::addColumn<int>("target");
        QTest::newRow("30-fps") << 30;
        QTest::newRow("60-fps") << 60;
        QTest::newRow("auto") << 0;
    }

    void submissionLatencyDoesNotMovePhase()
    {
        QFETCH(int, target);
        constexpr qint64 origin = 987'654'321'000'000;
        const long double period = periodNanoseconds(target, 60);
        const long double refreshPeriod = periodNanoseconds(0, 60);
        long double ideal = origin + period;
        qint64 swap = origin;
        qint64 previousRequest = 0;
        qint64 previousPresentation = origin;
        for (int frame = 1; frame <= 3000; ++frame) {
            // frameSwapped is submission, not presentation. It may occur
            // shortly after a request, before the predicted display time.
            // The 1.65 ms offset reproduces the observed 35/18 ms cadence
            // when each latest submission is used as a new phase origin.
            const qint64 now = swap + (frame == 1 ? 0 : 100'000);
            const auto next = schedule(now, ideal, target, 60, origin);
            const qint64 expectedRequest = std::llround(origin + frame * period - refreshPeriod);
            QCOMPARE(next.wakeNanoseconds, std::max(now, expectedRequest));
            QCOMPARE(next.presentationNanoseconds, std::llround(origin + frame * period));
            QVERIFY(next.presentationNanoseconds > previousPresentation);
            if (previousRequest) {
                QVERIFY(std::abs(next.wakeNanoseconds - previousRequest - period) < 2);
            }
            previousRequest = next.wakeNanoseconds;
            previousPresentation = next.presentationNanoseconds;
            ideal = next.nextTargetNanoseconds;
            swap = next.wakeNanoseconds + 1'650'000;
        }
    }

    void delayedSubmissionDoesNotAccumulate()
    {
        constexpr qint64 origin = 987'654'321'000'000;
        const long double period = periodNanoseconds(30, 60);
        const long double refreshPeriod = periodNanoseconds(0, 60);
        long double ideal = origin + period;
        qint64 swap = origin;
        qint64 previousSwap = 0;
        for (int frame = 1; frame <= 3000; ++frame) {
            const auto next = schedule(swap + 200'000, ideal, 30, 60, origin);
            // Even when submission comes after its predicted presentation,
            // don't add this backend latency to the following wake deadline.
            swap = next.wakeNanoseconds + std::llround(refreshPeriod) + 1'650'000;
            if (previousSwap) {
                QVERIFY(std::abs(swap - previousSwap - period) < 2);
            }
            previousSwap = swap;
            ideal = next.nextTargetNanoseconds;
        }
    }

    void clockUsesFixedPhase_data()
    {
        QTest::addColumn<int>("target");
        QTest::addColumn<int>("submissionDelayMs");
        QTest::newRow("30-late-submission") << 30 << 18;
        QTest::newRow("60-early-submission") << 60 << 2;
    }

    void clockUsesFixedPhase()
    {
        QFETCH(int, target);
        QFETCH(int, submissionDelayMs);
        QQuickWindow window; // No real rendering; supply synthetic swap feedback.
        PresentationClock clock(&window, target);
        QTimer submission;
        submission.setSingleShot(true);
        submission.setTimerType(Qt::PreciseTimer);
        submission.setInterval(submissionDelayMs);
        connect(&submission, &QTimer::timeout, &window, &QQuickWindow::frameSwapped);
        QEventLoop loop;
        QElapsedTimer elapsed;
        elapsed.start();
        std::vector<qint64> ticks;
        ticks.reserve(40);
        connect(&clock, &PresentationClock::frameTick, this, [&](qreal delta) {
            QVERIFY(delta > 0);
            ticks.push_back(elapsed.nsecsElapsed());
            if (ticks.size() == 40) {
                clock.setRunning(false);
                loop.quit();
            } else {
                submission.start();
            }
        });
        QTimer timeout;
        timeout.setSingleShot(true);
        connect(&timeout, &QTimer::timeout, &loop, &QEventLoop::quit);
        timeout.start(3000);
        clock.setRunning(true);
        loop.exec();
        clock.setRunning(false);
        submission.stop();
        QCOMPARE(ticks.size(), size_t(40));
        std::vector<qreal> intervals;
        for (size_t index = 5; index < ticks.size(); ++index) {
            intervals.push_back((ticks[index] - ticks[index - 1]) / 1'000'000.0);
        }
        std::sort(intervals.begin(), intervals.end());
        const qreal median = intervals[intervals.size() / 2];
        const qreal expected = 1000.0 / target;
        qInfo() << "Target FPS" << target << "median request interval ms" << median;
        // The former moving phase and future-presentation guard both add
        // ~2 ms per frame here, with periodic short corrections at 30 FPS.
        QVERIFY2(std::abs(median - expected) < 1.0, qPrintable(QString::number(median)));
    }

    void refreshChangesAndFeedbackRecovery()
    {
        constexpr qint64 origin = 1'000'000'000;
        const auto oldOutput = schedule(origin, origin + periodNanoseconds(0, 60), 0, 60, origin);
        QCOMPARE(oldOutput.presentationNanoseconds, origin + 16'666'667);

        // The clock rebases the ideal target to the first swap at the new rate.
        const qint64 newSwap = oldOutput.presentationNanoseconds;
        const auto newOutput = schedule(newSwap, newSwap + periodNanoseconds(0, 144), 0, 144, newSwap);
        QCOMPARE(newOutput.presentationNanoseconds - newSwap, 6'944'444);
        const auto capped = schedule(newSwap, newSwap + periodNanoseconds(30, 144), 30, 144, newSwap);
        QCOMPARE(capped.presentationNanoseconds - newSwap, 34'722'222);
        QVERIFY(std::abs(capped.presentationNanoseconds - (newSwap + periodNanoseconds(30, 144)))
                < periodNanoseconds(0, 144) / 2);

        const auto fallback = schedule(origin + 500'000'000, newOutput.nextTargetNanoseconds, 0, 144);
        QVERIFY(fallback.nextTargetNanoseconds > fallback.presentationNanoseconds);
        const qint64 recoveredSwap = fallback.presentationNanoseconds + 1'000'000;
        const auto recovered = schedule(recoveredSwap, recoveredSwap + periodNanoseconds(0, 75),
                                        0, 75, recoveredSwap);
        QCOMPARE(recovered.presentationNanoseconds - recoveredSwap, 13'333'333);
    }

    void invalidRefreshFallsBackTo60()
    {
        for (qreal refresh : {0.0, -1.0, std::numeric_limits<qreal>::quiet_NaN(),
                             std::numeric_limits<qreal>::infinity()}) {
            QCOMPARE(PresentationPacing::validRefreshRate(refresh), 60.0);
            QCOMPARE(periodNanoseconds(0, refresh), periodNanoseconds(0, 60));
            QCOMPARE(schedule(0, periodNanoseconds(30, refresh), 30, refresh, 0).presentationNanoseconds,
                     33'333'333);
        }
    }

    void synchronousWorkDoesNotExtendTimerPeriod()
    {
        // An unexposed window has no scene-graph swaps, exercising the actual
        // fallback timer rather than just the scheduling calculation.
        QQuickWindow window;
        PresentationClock clock(&window, 30);
        QElapsedTimer elapsed;
        elapsed.start();
        std::vector<qint64> ticks;
        ticks.reserve(8);
        connect(&clock, &PresentationClock::frameTick, this, [&](qreal delta) {
            QVERIFY(delta > 0);
            ticks.push_back(elapsed.nsecsElapsed());
            if (ticks.size() == 8) {
                clock.setRunning(false);
            } else {
                QTest::qSleep(12); // Synchronous simulation/render preparation.
            }
        });
        clock.setRunning(true);
        QTRY_COMPARE_WITH_TIMEOUT(ticks.size(), size_t(8), 2000);
        const qreal measuredMilliseconds = (ticks.back() - ticks.front()) / 1'000'000.0;
        // Seven 33.3 ms periods = 233.3 ms. Rearming after the work would take
        // at least 317 ms; allow OS scheduling jitter without masking that bug.
        QVERIFY2(measuredMilliseconds >= 210 && measuredMilliseconds < 285,
                 qPrintable(QString::number(measuredMilliseconds)));
        QTest::qWait(60);
        QCOMPARE(ticks.size(), size_t(8));
    }

    void stopResumeAndSharedListeners()
    {
        QQuickWindow window;
        PresentationClock clock(&window, 30);
        QSignalSpy firstListener(&clock, &PresentationClock::frameTick);
        QSignalSpy secondListener(&clock, &PresentationClock::frameTick);
        clock.setRunning(true);
        QTRY_VERIFY(firstListener.count() >= 3);
        clock.setRunning(false);
        const int stoppedCount = firstListener.count();
        QTest::qWait(150);
        QCOMPARE(firstListener.count(), stoppedCount);
        QCOMPARE(secondListener.count(), stoppedCount);
        QElapsedTimer resumeDuration;
        resumeDuration.start();
        clock.setRunning(true);
        const qreal resumeSeconds = resumeDuration.nsecsElapsed() / 1'000'000'000.0;
        // Resume emits the bootstrap tick synchronously. Its delta includes
        // one nominal period plus time spent resuming, never the paused time.
        QCOMPARE(firstListener.count(), stoppedCount + 1);
        clock.setRunning(false);
        QCOMPARE(firstListener.count(), secondListener.count());
        const qreal resumedDelta = firstListener.at(stoppedCount).at(0).toReal();
        const qreal refresh = window.screen() ? window.screen()->refreshRate() : 60.0;
        const qreal periodSeconds = periodNanoseconds(30, refresh) / 1'000'000'000.0L;
        QVERIFY(resumedDelta >= periodSeconds - 1e-9);
        QVERIFY(resumedDelta <= periodSeconds + resumeSeconds + 1e-9);
        for (int index = 0; index < firstListener.count(); ++index) {
            QCOMPARE(firstListener.at(index).at(0), secondListener.at(index).at(0));
        }
    }

    void targetChangesKeepPresentationTimeMonotonic()
    {
        QQuickWindow window;
        PresentationClock clock(&window, 30);
        QSignalSpy ticks(&clock, &PresentationClock::frameTick);
        QSignalSpy presentations(&clock, &PresentationClock::presentationTick);
        clock.setRunning(true);
        QTRY_VERIFY(ticks.count() >= 3);
        clock.setTargetFrameRate(60);
        const int changedAt = ticks.count();
        QTRY_VERIFY(ticks.count() >= changedAt + 8);
        clock.setRunning(false);
        QCOMPARE(ticks.count(), presentations.count());
        for (int i = 1; i < presentations.count(); ++i) {
            const qint64 previous = presentations.at(i - 1).at(0).toLongLong();
            const qint64 current = presentations.at(i).at(0).toLongLong();
            QVERIFY(current > previous);
            QVERIFY(std::abs(ticks.at(i).at(0).toDouble() - (current - previous) / 1e9) < 1e-9);
        }
        const int end = presentations.count() - 1;
        const double mean = (presentations.at(end).at(0).toLongLong()
            - presentations.at(end - 4).at(0).toLongLong()) / 4e9;
        QVERIFY(mean > 0.014 && mean < 0.020);
    }

    void renderedFeedbackIncludesStallTime()
    {
        // Exercise real Qt Quick swap callbacks too. The offscreen software
        // backend does not provide physical vsync; the virtual KWin harness
        // remains the authoritative test of display-aligned presentation.
        QQuickWindow window;
        window.resize(64, 64);
        PresentationClock clock(&window, 30);
        QElapsedTimer elapsed;
        elapsed.start();
        std::vector<qint64> swaps;
        std::vector<qreal> deltas;
        swaps.reserve(32);
        deltas.reserve(14);
        connect(&window, &QQuickWindow::frameSwapped, this, [&] {
            swaps.push_back(elapsed.nsecsElapsed());
        });
        connect(&clock, &PresentationClock::frameTick, this, [&](qreal delta) {
            deltas.push_back(delta);
            window.setColor(deltas.size() % 2 ? Qt::red : Qt::blue);
            if (deltas.size() == 8) {
                QTest::qSleep(160);
            } else if (deltas.size() == 14) {
                clock.setRunning(false);
            }
        });
        window.show();
        clock.setRunning(true);
        QTRY_COMPARE_WITH_TIMEOUT(deltas.size(), size_t(14), 4000);
        clock.setRunning(false);
        window.hide();
        QVERIFY(swaps.size() >= 10);
        // The tick predicts presentation one refresh ahead. On watchdog
        // fallback it can lose that lead, but must still include the stall
        // instead of clipping it to the former 100 ms limit.
        QVERIFY2(*std::max_element(deltas.begin(), deltas.end()) >= 0.12,
                 qPrintable(QString::number(*std::max_element(deltas.begin(), deltas.end()))));
        std::vector<qreal> steadyIntervals;
        for (size_t index = 3; index < swaps.size(); ++index) {
            const qreal milliseconds = (swaps[index] - swaps[index - 1]) / 1'000'000.0;
            if (milliseconds < 100) steadyIntervals.push_back(milliseconds);
        }
        QVERIFY(steadyIntervals.size() >= 6);
        std::sort(steadyIntervals.begin(), steadyIntervals.end());
        const qreal median = steadyIntervals[steadyIntervals.size() / 2];
        QVERIFY2(median >= 29 && median < 39, qPrintable(QString::number(median)));
    }
};

QTEST_MAIN(PresentationClockTest)
#include "test_presentationclock.moc"

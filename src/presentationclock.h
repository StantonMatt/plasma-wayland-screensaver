// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once

#include <QChronoTimer>
#include <QObject>
#include <atomic>
#include <memory>
#include <optional>

namespace PresentationPacing {
// All times use the same monotonic nanosecond clock. Keeping the ideal target
// separate from the selected vsync prevents rounding and work-time drift.
struct Schedule {
    qint64 wakeNanoseconds;
    qint64 presentationNanoseconds;
    long double nextTargetNanoseconds;
};

qreal validRefreshRate(qreal refreshRate);
// Smallest whole number of refreshes whose rate does not exceed the cap.
// Auto has no cap here; OverlayManager retains the snakes-specific 60 FPS cap.
int refreshDivisor(int targetFrameRate, qreal refreshRate);
long double periodNanoseconds(int targetFrameRate, qreal refreshRate);
// The phase anchor stays fixed between bootstrap, refresh changes and feedback
// recovery. A later submission timestamp must not replace it each frame.
Schedule schedule(qint64 nowNanoseconds, long double nextTargetNanoseconds,
                  int targetFrameRate, qreal refreshRate,
                  std::optional<qint64> phaseAnchorNanoseconds = std::nullopt);
}

class PresentationTraceState;

class QQuickWindow;

class PresentationClock final : public QObject
{
    Q_OBJECT
    friend class PresentationClockTest;

public:
    explicit PresentationClock(QQuickWindow *window, int targetFrameRate,
                               QObject *parent = nullptr);

    ~PresentationClock() override;
    void setRunning(bool running);
    void setTargetFrameRate(int targetFrameRate);
    void setTraceSimulationSource(QObject *source);

Q_SIGNALS:
    void frameTick(qreal deltaSeconds);
    // Absolute CLOCK_MONOTONIC time; shared worlds must not sum window deltas.
    void presentationTick(qint64 presentationNanoseconds);

private:
    void handleFrameSwapped();
    void scheduleNextFrame();
    void presentNextFrame();
    void tickAndRequestUpdate(qint64 presentationNanoseconds);
    void updateRefreshRate(qint64 nowNanoseconds);
    void armTimer(qint64 deadlineNanoseconds);

    std::shared_ptr<PresentationTraceState> m_trace;
    // frameSwapped can originate on the render thread. Capture its time there,
    // rather than measuring when the queued GUI callback eventually runs.
    std::shared_ptr<std::atomic<qint64>> m_swapTimestamp;
    // All remaining pacing/prediction state and public methods are GUI-owned.
    // Render-thread callbacks capture independent shared state, never this.
    QQuickWindow *m_window;
    QChronoTimer m_wakeTimer;
    int m_targetFrameRate;
    qreal m_refreshRate = 60.0;
    long double m_nextTargetNanoseconds = 0;
    std::optional<qint64> m_lastSwapNanoseconds;
    // frameSwapped measures submission, with request/render/backend latency.
    // Keep the phase origin fixed rather than adding that latency every frame.
    std::optional<qint64> m_phaseAnchorNanoseconds;
    qint64 m_wakeDeadlineNanoseconds = 0;
    qint64 m_lastSwapCallbackNanoseconds = 0;
    qint64 m_timerFallbacks = 0;
    qint64 m_lastTickPresentationNanoseconds = 0;
    qint64 m_runStartNanoseconds = 0;
    qint64 m_lastObservedSwapNanoseconds = 0;
    bool m_waitingForSwap = false;
    bool m_running = false;
};

// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once

#include <QChronoTimer>
#include <QElapsedTimer>
#include <QObject>
#include <memory>

class PresentationTraceState;

class QQuickWindow;

class PresentationClock final : public QObject
{
    Q_OBJECT

public:
    explicit PresentationClock(QQuickWindow *window, int targetFrameRate,
                               QObject *parent = nullptr);

    ~PresentationClock() override;
    void setRunning(bool running);
    void setTraceSimulationSource(QObject *source);

Q_SIGNALS:
    void frameTick(qreal deltaSeconds);

private:
    void handleFrameSwapped();
    void scheduleNextFrame();
    void presentNextFrame();
    void tickAndRequestUpdate(qreal fallbackSeconds = 0.0);

    std::shared_ptr<PresentationTraceState> m_trace;
    QQuickWindow *m_window;
    QChronoTimer m_wakeTimer;
    QElapsedTimer m_elapsed;
    int m_targetFrameRate;
    bool m_running = false;
};

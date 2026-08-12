// SPDX-License-Identifier: GPL-3.0-or-later
#include "presentationclock.h"

#include <QQuickWindow>
#include <QScreen>

#include <algorithm>
#include <chrono>

PresentationClock::PresentationClock(QQuickWindow *window, int targetFrameRate, QObject *parent)
    : QObject(parent)
    , m_window(window)
    , m_targetFrameRate(targetFrameRate)
{
    m_wakeTimer.setSingleShot(true);
    m_wakeTimer.setTimerType(Qt::PreciseTimer);
    connect(&m_wakeTimer, &QChronoTimer::timeout, this, &PresentationClock::presentNextFrame);
    connect(m_window, &QQuickWindow::frameSwapped, this,
            &PresentationClock::handleFrameSwapped, Qt::QueuedConnection);
}

void PresentationClock::setRunning(bool running)
{
    if (m_running == running) {
        return;
    }
    m_running = running;
    m_wakeTimer.stop();
    m_elapsed.invalidate();
    if (m_running) {
        m_elapsed.start();
        if (m_targetFrameRate <= 0) {
            // Bootstrap by dirtying animated state once. Subsequent automatic
            // ticks are chained exclusively from completed presentations.
            QMetaObject::invokeMethod(this, [this] {
                const qreal refreshRate = m_window->screen()
                    ? m_window->screen()->refreshRate() : 60.0;
                tickAndRequestUpdate(1.0 / std::max(1.0, refreshRate));
            }, Qt::QueuedConnection);
        } else {
            scheduleNextFrame();
        }
    }
}

void PresentationClock::handleFrameSwapped()
{
    if (m_running && m_targetFrameRate <= 0) {
        tickAndRequestUpdate();
    }
}

void PresentationClock::presentNextFrame()
{
    if (!m_running) {
        return;
    }
    tickAndRequestUpdate();
    scheduleNextFrame();
}

void PresentationClock::tickAndRequestUpdate(qreal fallbackSeconds)
{
    qreal deltaSeconds = m_elapsed.isValid()
        ? std::min(m_elapsed.nsecsElapsed() / 1'000'000'000.0, 0.10) : 0.0;
    m_elapsed.restart();
    if (deltaSeconds <= 0.0001) {
        deltaSeconds = fallbackSeconds;
    }
    if (deltaSeconds > 0.0) {
        Q_EMIT frameTick(deltaSeconds);
    }
    m_window->requestUpdate();
}

void PresentationClock::scheduleNextFrame()
{
    const qreal refreshRate = m_window->screen() ? m_window->screen()->refreshRate() : 60.0;
    const qreal requestedRate = std::min<qreal>(m_targetFrameRate, refreshRate);
    // QChronoTimer provides sub-millisecond scheduling for 144–240 Hz output.
    // Fixed caps use a precise wakeup, but motion still advances from measured
    // elapsed time so delayed frames do not slow the apparent motion.
    const auto period = std::chrono::duration_cast<std::chrono::nanoseconds>(
        std::chrono::duration<qreal>(1.0 / std::max(1.0, requestedRate)));
    m_wakeTimer.setInterval(period);
    m_wakeTimer.start();
}

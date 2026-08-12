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
    // frameSwapped can originate from a render thread. Queue the update to the
    // GUI thread where QML properties and QWindow::requestUpdate belong.
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
        m_window->requestUpdate();
    }
}

void PresentationClock::handleFrameSwapped()
{
    if (!m_running) {
        return;
    }
    scheduleNextFrame();
}

void PresentationClock::presentNextFrame()
{
    if (!m_running) {
        return;
    }
    const qreal deltaSeconds = m_elapsed.isValid()
        ? std::min(m_elapsed.nsecsElapsed() / 1'000'000'000.0, 0.10) : 0.0;
    m_elapsed.restart();
    if (deltaSeconds > 0.0) {
        Q_EMIT framePresented(deltaSeconds);
    }
    // Updating QML in framePresented normally schedules a frame itself, while
    // requestUpdate() guarantees progress for animations with cached content.
    m_window->requestUpdate();
}

void PresentationClock::scheduleNextFrame()
{
    const qreal refreshRate = m_window->screen() ? m_window->screen()->refreshRate() : 60.0;
    const qreal requestedRate = m_targetFrameRate <= 0
        ? refreshRate : std::min<qreal>(m_targetFrameRate, refreshRate);
    // QChronoTimer provides sub-millisecond scheduling for 144–240 Hz output.
    // It only schedules requestUpdate(); animation time still comes from the
    // actual frameSwapped cadence rather than from this timer.
    const auto period = std::chrono::duration_cast<std::chrono::nanoseconds>(
        std::chrono::duration<qreal>(1.0 / std::max(1.0, requestedRate)));
    const auto elapsed = m_elapsed.isValid()
        ? std::chrono::nanoseconds(m_elapsed.nsecsElapsed()) : std::chrono::nanoseconds::zero();
    // Subtract time already spent polishing/rendering/presenting this frame so
    // those costs do not lower the requested cadence at high refresh rates.
    m_wakeTimer.setInterval(std::max(std::chrono::nanoseconds::zero(), period - elapsed));
    m_wakeTimer.start();
}

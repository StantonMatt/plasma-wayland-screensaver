// SPDX-License-Identifier: GPL-3.0-or-later
#include "idlemonitor.h"

#include <KIdleTime>

IdleMonitor::IdleMonitor(QObject *parent)
    : QObject(parent)
    , m_idleTime(KIdleTime::instance())
    , m_setResumeWatching([this](bool watching) {
        if (watching) m_idleTime->catchNextResumeEvent();
        else m_idleTime->stopCatchingResumeEvent();
    })
{
    m_freshIntervalTimer.setSingleShot(true);
    m_freshIntervalTimer.setTimerType(Qt::PreciseTimer);
    connect(&m_freshIntervalTimer, &QTimer::timeout, this, &IdleMonitor::checkIdleTimeout);
    connect(m_idleTime, &KIdleTime::timeoutReached, this, [this](int identifier, int) {
        if (m_timeoutId >= 0 && identifier == m_timeoutId) {
            m_backendIdle = true;
            checkIdleTimeout();
        }
    });
    connect(m_idleTime, &KIdleTime::resumingFromIdle, this, [this] {
        m_backendIdle = false;
        if (m_freshInterval) {
            m_freshIntervalTimer.start();
            rearmResumeWatch();
        }
        Q_EMIT activityResumed();
    });
}

IdleMonitor::~IdleMonitor()
{
    stop();
}

void IdleMonitor::start(int timeoutMilliseconds, bool freshInterval)
{
    // Configuration saves and teardown completion may schedule the same
    // interval again. Preserve the timer and backend idle state in that case.
    if (m_timeoutMilliseconds == timeoutMilliseconds && m_freshInterval == freshInterval) return;
    stop();
    m_timeoutMilliseconds = timeoutMilliseconds;
    m_freshInterval = freshInterval;
    if (freshInterval) {
        // A newly registered compositor timeout can already be expired. Require
        // a complete local interval as well, reset by every observed resume.
        m_freshIntervalTimer.start(timeoutMilliseconds);
        rearmResumeWatch();
    }
    m_timeoutId = m_idleTime->addIdleTimeout(timeoutMilliseconds);
}

void IdleMonitor::stop()
{
    ++m_resumeWatchGeneration;
    m_timeoutMilliseconds = -1;
    m_freshIntervalTimer.stop();
    m_freshInterval = false;
    m_backendIdle = false;
    if (m_timeoutId >= 0) {
        m_idleTime->removeIdleTimeout(m_timeoutId);
        m_timeoutId = -1;
    }
    m_setResumeWatching(false);
}

void IdleMonitor::watchForResume()
{
    stop();
    rearmResumeWatch();
}

void IdleMonitor::rearmResumeWatch()
{
    // KIdleTime cancels catching *after* emitting resumingFromIdle. All callers,
    // including activityResumed subscribers that start or watch again, must
    // wait until that dispatch has unwound. stop()/a newer start invalidate it.
    const quint64 generation = m_resumeWatchGeneration;
    QMetaObject::invokeMethod(this, [this, generation] {
        if (generation == m_resumeWatchGeneration) m_setResumeWatching(true);
    }, Qt::QueuedConnection);
}

void IdleMonitor::checkIdleTimeout()
{
    if (m_timeoutId >= 0 && m_backendIdle && !m_freshIntervalTimer.isActive()) {
        Q_EMIT idleTimeoutReached();
    }
}

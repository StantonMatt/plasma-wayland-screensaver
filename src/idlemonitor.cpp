// SPDX-License-Identifier: GPL-3.0-or-later
#include "idlemonitor.h"

#include <KIdleTime>

IdleMonitor::IdleMonitor(QObject *parent)
    : QObject(parent)
    , m_idleTime(KIdleTime::instance())
{
    connect(m_idleTime, &KIdleTime::timeoutReached, this, [this](int identifier, int) {
        if (identifier == m_timeoutId) {
            Q_EMIT idleTimeoutReached();
        }
    });
    connect(m_idleTime, &KIdleTime::resumingFromIdle, this, &IdleMonitor::activityResumed);
}

void IdleMonitor::start(int timeoutMilliseconds)
{
    if (m_timeoutId >= 0) {
        m_idleTime->removeIdleTimeout(m_timeoutId);
    }
    m_idleTime->stopCatchingResumeEvent();
    m_timeoutId = m_idleTime->addIdleTimeout(timeoutMilliseconds);
}

void IdleMonitor::stop()
{
    if (m_timeoutId >= 0) {
        m_idleTime->removeIdleTimeout(m_timeoutId);
        m_timeoutId = -1;
    }
    m_idleTime->stopCatchingResumeEvent();
}

void IdleMonitor::watchForResume()
{
    m_idleTime->catchNextResumeEvent();
}

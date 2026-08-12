// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once

#include <QObject>

class KIdleTime;

class IdleMonitor final : public QObject
{
    Q_OBJECT

public:
    explicit IdleMonitor(QObject *parent = nullptr);
    void start(int timeoutMilliseconds);
    void stop();
    void watchForResume();

Q_SIGNALS:
    void idleTimeoutReached();
    void activityResumed();

private:
    KIdleTime *m_idleTime = nullptr;
    int m_timeoutId = -1;
};

// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once

#include <QObject>
#include <QTimer>
#include <functional>

class KIdleTime;

class IdleMonitor final : public QObject
{
    Q_OBJECT
    friend class ApplicationControllerTest;

public:
    explicit IdleMonitor(QObject *parent = nullptr);
    ~IdleMonitor() override;
    void start(int timeoutMilliseconds, bool freshInterval = false);
    void stop();
    void watchForResume();
    void checkIdleTimeout();

Q_SIGNALS:
    void idleTimeoutReached();
    void activityResumed();

private:
    void rearmResumeWatch();

    KIdleTime *m_idleTime = nullptr;
    // Replaceable by the offscreen tests, whose KIdleTime has no poller.
    std::function<void(bool)> m_setResumeWatching;
    quint64 m_resumeWatchGeneration = 0;
    int m_timeoutId = -1;
    int m_timeoutMilliseconds = -1;
    QTimer m_freshIntervalTimer;
    bool m_freshInterval = false;
    bool m_backendIdle = false;
};

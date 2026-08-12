// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once

#include <QObject>

class ScreensaverStateMachine final : public QObject
{
    Q_OBJECT

public:
    enum class State { Waiting, Activating, Active };
    Q_ENUM(State)

    explicit ScreensaverStateMachine(QObject *parent = nullptr);

    [[nodiscard]] State state() const;
    [[nodiscard]] bool isActive() const;

public Q_SLOTS:
    void idleTimeoutReached();
    void previewRequested();
    void activationSucceeded();
    void activationFailed();
    void activityDetected();
    void stop();

Q_SIGNALS:
    void activationRequested(bool preview);
    void dismissalRequested();
    void stateChanged(ScreensaverStateMachine::State state);

private:
    void setState(State state);
    State m_state = State::Waiting;
};

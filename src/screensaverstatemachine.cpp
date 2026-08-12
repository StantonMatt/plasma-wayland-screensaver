// SPDX-License-Identifier: GPL-3.0-or-later
#include "screensaverstatemachine.h"

ScreensaverStateMachine::ScreensaverStateMachine(QObject *parent)
    : QObject(parent)
{
}

ScreensaverStateMachine::State ScreensaverStateMachine::state() const
{
    return m_state;
}

bool ScreensaverStateMachine::isActive() const
{
    return m_state == State::Active || m_state == State::Activating;
}

void ScreensaverStateMachine::idleTimeoutReached()
{
    if (m_state != State::Waiting) {
        return;
    }
    setState(State::Activating);
    Q_EMIT activationRequested(false);
}

void ScreensaverStateMachine::previewRequested()
{
    if (m_state != State::Waiting) {
        return;
    }
    setState(State::Activating);
    Q_EMIT activationRequested(true);
}

void ScreensaverStateMachine::activationSucceeded()
{
    if (m_state == State::Activating) {
        setState(State::Active);
    }
}

void ScreensaverStateMachine::activationFailed()
{
    if (m_state == State::Activating) {
        setState(State::Waiting);
    }
}

void ScreensaverStateMachine::activityDetected()
{
    if (m_state == State::Waiting) {
        return;
    }
    setState(State::Waiting);
    Q_EMIT dismissalRequested();
}

void ScreensaverStateMachine::stop()
{
    activityDetected();
}

void ScreensaverStateMachine::setState(State state)
{
    if (state == m_state) {
        return;
    }
    m_state = state;
    Q_EMIT stateChanged(state);
}

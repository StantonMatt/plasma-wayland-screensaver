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
    if (m_state == State::Dismissing) {
        m_previewPending = true;
        return;
    }
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
        activityDetected();
    }
}

void ScreensaverStateMachine::activityDetected()
{
    if (m_state == State::Waiting || m_state == State::Dismissing) {
        return;
    }
    setState(State::Dismissing);
    Q_EMIT dismissalRequested();
}

void ScreensaverStateMachine::stop()
{
    m_previewPending = false;
    activityDetected();
}

void ScreensaverStateMachine::teardownCompleted()
{
    if (m_state != State::Dismissing) return;
    const bool preview = m_previewPending;
    m_previewPending = false;
    setState(State::Waiting);
    if (preview) previewRequested();
}

void ScreensaverStateMachine::setState(State state)
{
    if (state == m_state) {
        return;
    }
    m_state = state;
    Q_EMIT stateChanged(state);
}

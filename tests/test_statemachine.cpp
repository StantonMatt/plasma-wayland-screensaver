// SPDX-License-Identifier: GPL-3.0-or-later
#include "../src/screensaverstatemachine.h"

#include <QSignalSpy>
#include <QTest>

class StateMachineTest final : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void idleActivationAndDismissal()
    {
        ScreensaverStateMachine machine;
        QSignalSpy activation(&machine, &ScreensaverStateMachine::activationRequested);
        QSignalSpy dismissal(&machine, &ScreensaverStateMachine::dismissalRequested);

        machine.idleTimeoutReached();
        QCOMPARE(machine.state(), ScreensaverStateMachine::State::Activating);
        QCOMPARE(activation.count(), 1);
        QCOMPARE(activation.first().first().toBool(), false);

        machine.activationSucceeded();
        QCOMPARE(machine.state(), ScreensaverStateMachine::State::Active);
        machine.activityDetected();
        QCOMPARE(machine.state(), ScreensaverStateMachine::State::Waiting);
        QCOMPARE(dismissal.count(), 1);
    }

    void previewAndFailure()
    {
        ScreensaverStateMachine machine;
        QSignalSpy activation(&machine, &ScreensaverStateMachine::activationRequested);
        machine.previewRequested();
        QCOMPARE(activation.count(), 1);
        QCOMPARE(activation.first().first().toBool(), true);
        machine.activationFailed();
        QCOMPARE(machine.state(), ScreensaverStateMachine::State::Waiting);
    }

    void ignoresDuplicateTriggers()
    {
        ScreensaverStateMachine machine;
        QSignalSpy activation(&machine, &ScreensaverStateMachine::activationRequested);
        machine.idleTimeoutReached();
        machine.idleTimeoutReached();
        machine.previewRequested();
        QCOMPARE(activation.count(), 1);
    }
};

QTEST_GUILESS_MAIN(StateMachineTest)
#include "test_statemachine.moc"

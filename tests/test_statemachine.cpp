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
        QCOMPARE(machine.state(), ScreensaverStateMachine::State::Dismissing);
        QCOMPARE(dismissal.count(), 1);
        machine.idleTimeoutReached();
        machine.activityDetected();
        QCOMPARE(activation.count(), 1);
        QCOMPARE(dismissal.count(), 1);
        machine.teardownCompleted();
        QCOMPARE(machine.state(), ScreensaverStateMachine::State::Waiting);
    }

    void previewAndFailure()
    {
        ScreensaverStateMachine machine;
        QSignalSpy activation(&machine, &ScreensaverStateMachine::activationRequested);
        machine.previewRequested();
        QCOMPARE(activation.count(), 1);
        QCOMPARE(activation.first().first().toBool(), true);
        machine.activationFailed();
        QCOMPARE(machine.state(), ScreensaverStateMachine::State::Dismissing);
        machine.teardownCompleted();
        QCOMPARE(machine.state(), ScreensaverStateMachine::State::Waiting);
    }

    void defersPreviewUntilTeardownCompletes()
    {
        ScreensaverStateMachine machine;
        QSignalSpy activation(&machine, &ScreensaverStateMachine::activationRequested);
        machine.previewRequested();
        machine.activationSucceeded();
        machine.activityDetected();
        machine.previewRequested();
        machine.previewRequested(); // Coalesce repeated explicit requests.
        machine.idleTimeoutReached();
        QCOMPARE(activation.count(), 1);
        QCOMPARE(machine.state(), ScreensaverStateMachine::State::Dismissing);
        machine.teardownCompleted();
        QCOMPARE(activation.count(), 2);
        QVERIFY(activation.last().first().toBool());
        QCOMPARE(machine.state(), ScreensaverStateMachine::State::Activating);
        machine.teardownCompleted(); // Stale completion cannot re-activate.
        QCOMPARE(activation.count(), 2);
    }

    void stopCancelsDeferredPreview()
    {
        ScreensaverStateMachine machine;
        QSignalSpy activation(&machine, &ScreensaverStateMachine::activationRequested);
        machine.previewRequested();
        machine.activityDetected();
        machine.previewRequested();
        machine.stop();
        machine.teardownCompleted();
        QCOMPARE(machine.state(), ScreensaverStateMachine::State::Waiting);
        QCOMPARE(activation.count(), 1);
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

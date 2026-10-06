// SPDX-License-Identifier: GPL-3.0-or-later
#include "applicationcontroller.h"
#include <KIdleTime>
#include <QDesktopServices>
#include <QFile>
#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQmlComponent>
#include <QQuickWindow>
#include <QScopeGuard>
#include <QSignalSpy>
#include <QTemporaryDir>
#include <QTest>
#include <QUrl>

class ApplicationControllerTest final : public QObject
{
    Q_OBJECT
    QUrl m_openedRelease;
    int m_releaseCount = 0;

    static void observeResumeWatching(IdleMonitor &monitor, bool &watching, int &armCount)
    {
        const auto setWatching = monitor.m_setResumeWatching;
        monitor.m_setResumeWatching = [setWatching, &watching, &armCount](bool enabled) {
            setWatching(enabled);
            watching = enabled;
            if (enabled) ++armCount;
        };
    }

    static void backendResume(IdleMonitor &monitor)
    {
        Q_EMIT KIdleTime::instance()->resumingFromIdle();
        // Mirror KIdleTime's private handler: cancellation happens after all
        // direct resume subscribers return. The probe observes the real calls,
        // even though the offscreen platform has no poller to deliver resumes.
        monitor.m_setResumeWatching(false);
    }
public Q_SLOTS:
    void captureReleasePage(const QUrl &url)
    {
        m_openedRelease = url;
        ++m_releaseCount;
    }
private Q_SLOTS:
    void settingsQmlDependenciesAreAvailable()
    {
        QQmlEngine engine;
        QQmlComponent component(&engine);
        component.setData("import QtQuick\nimport QtQuick.Shapes\n"
            "import org.kde.kirigami as Kirigami\nimport org.kde.kcmutils as KCMUtils\n"
            "import org.kde.kirigamiaddons.delegates as Delegates\nItem {}", QUrl());
        QVERIFY2(component.isReady(), qPrintable(component.errorString()));
        std::unique_ptr<QObject> item(component.create());
        QVERIFY(item);
    }

    void reportsRealInhibitorFailureOnlyForPreview()
    {
        ApplicationController controller;
        QSignalSpy failed(&controller, &ApplicationController::previewFailed);
        controller.Preview();
        QTRY_COMPARE(failed.count(), 1);
        QCOMPARE(failed.constFirst().constFirst().toString(), QStringLiteral("The display couldn't be kept awake."));
        QVERIFY(!controller.screensaverActive());
        Q_EMIT controller.m_inhibitor.failed(QStringLiteral("stale response"));
        QCOMPARE(failed.count(), 1);
        QTRY_COMPARE(controller.m_stateMachine.state(), ScreensaverStateMachine::State::Waiting);
        controller.m_stateMachine.idleTimeoutReached();
        QTRY_COMPARE(controller.m_stateMachine.state(), ScreensaverStateMachine::State::Waiting);
        QCOMPARE(failed.count(), 1);
        controller.Preview();
        QTRY_COMPARE(failed.count(), 2);
    }

    void mapsTechnicalInhibitorErrors_data()
    {
        QTest::addColumn<QString>("error");
        QTest::newRow("service-unavailable") << QStringLiteral("org.freedesktop.DBus.Error.ServiceUnknown: PowerManagement unavailable");
        QTest::newRow("service-timeout") << QStringLiteral("org.freedesktop.DBus.Error.NoReply: Did not receive a reply");
        QTest::newRow("portal-subscription") << QStringLiteral("Could not subscribe to the portal inhibition response; fallback failed");
        QTest::newRow("portal-returned-subscription") << QStringLiteral("Could not subscribe to the returned portal inhibition request; fallback failed");
        QTest::newRow("portal-denied") << QStringLiteral("Portal inhibition request failed with response 2; fallback failed");
        QTest::newRow("power-call-error") << QStringLiteral("org.freedesktop.DBus.Error.AccessDenied: Inhibit rejected");
        QTest::newRow("empty-error") << QString();
    }

    void mapsTechnicalInhibitorErrors()
    {
        QFETCH(QString, error);
        ApplicationController controller;
        disconnect(&controller.m_stateMachine, &ScreensaverStateMachine::activationRequested,
                   &controller, &ApplicationController::activate);
        controller.m_activationIsPreview = true;
        controller.m_stateMachine.previewRequested();
        QSignalSpy failed(&controller, &ApplicationController::previewFailed);
        Q_EMIT controller.m_inhibitor.failed(error);
        QCOMPARE(failed.count(), 1);
        QCOMPARE(failed.constFirst().constFirst().toString(), QStringLiteral("The display couldn't be kept awake."));
        QVERIFY(!controller.screensaverActive());
    }

    void busyPreviewReportsFailure_data()
    {
        QTest::addColumn<bool>("active");
        QTest::addColumn<bool>("debug");
        QTest::newRow("starting-preview") << false << false;
        QTest::newRow("active-preview") << true << false;
        QTest::newRow("starting-debug") << false << true;
        QTest::newRow("active-debug") << true << true;
    }

    void busyPreviewReportsFailure()
    {
        QFETCH(bool, active);
        QFETCH(bool, debug);
        ApplicationController controller;
        disconnect(&controller.m_stateMachine, &ScreensaverStateMachine::activationRequested,
                   &controller, &ApplicationController::activate);
        controller.m_stateMachine.idleTimeoutReached();
        if (active) controller.m_stateMachine.activationSucceeded();
        const auto state = controller.m_stateMachine.state();
        QSignalSpy failed(&controller, &ApplicationController::previewFailed);
        if (debug) controller.PreviewDebug();
        else controller.Preview();
        QCOMPARE(failed.count(), 1);
        QCOMPARE(failed.constFirst().constFirst().toString(), QStringLiteral("The screensaver is already running."));
        QCOMPARE(controller.m_stateMachine.state(), state);
        QVERIFY(!controller.m_debugPreviewPending);
    }

    void logsActivationSourceAndDismissalReason_data()
    {
        QTest::addColumn<bool>("preview");
        QTest::addColumn<QString>("reason");
        QTest::newRow("preview-input") << true << QStringLiteral("input");
        QTest::newRow("idle-input") << false << QStringLiteral("input");
        QTest::newRow("output-change") << false << QStringLiteral("output change");
        QTest::newRow("resume") << false << QStringLiteral("idle resume");
        QTest::newRow("quit") << true << QStringLiteral("quit");
        QTest::newRow("failure") << true << QStringLiteral("activation failure");
    }

    void logsActivationSourceAndDismissalReason()
    {
        QFETCH(bool, preview);
        QFETCH(QString, reason);
        ApplicationController controller;
        // Keep the real activation wiring and logging. No desktop inhibition
        // is available on this test's private, nonexistent session bus.
        disconnect(&controller.m_inhibitor, &Inhibitor::failed, &controller, nullptr);
        QTest::ignoreMessage(QtInfoMsg, preview ? "Activating screensaver (source: preview)"
                                               : "Activating screensaver (source: idle)");
        if (preview) controller.Preview();
        else controller.m_stateMachine.idleTimeoutReached();
        QCOMPARE(controller.m_stateMachine.state(), ScreensaverStateMachine::State::Activating);
        if (reason != QStringLiteral("activation failure")) {
            controller.finishActivation();
        }
        const QByteArray message = QStringLiteral("Dismissing screensaver (reason: %1)").arg(reason).toUtf8();
        QTest::ignoreMessage(QtInfoMsg, message.constData());
        if (reason == QStringLiteral("input")) Q_EMIT controller.m_overlays.inputDetected();
        else if (reason == QStringLiteral("output change")) Q_EMIT controller.m_overlays.overlayUnavailable();
        else if (reason == QStringLiteral("idle resume")) controller.requestDismissal("idle resume");
        else if (reason == QStringLiteral("quit")) controller.requestDismissal("quit");
        else controller.failActivation(QStringLiteral("test failure"), QStringLiteral("test failure"));
        QCOMPARE(controller.m_stateMachine.state(), ScreensaverStateMachine::State::Dismissing);
        QTRY_COMPARE(controller.m_stateMachine.state(), ScreensaverStateMachine::State::Waiting);
        controller.m_idleMonitor.stop();
    }

    void dismissalGatesIdleAndPreview_data()
    {
        QTest::addColumn<bool>("preview");
        QTest::addColumn<bool>("debug");
        QTest::newRow("idle") << false << false;
        QTest::newRow("preview") << true << false;
        QTest::newRow("debug-preview") << true << true;
    }

    void dismissalGatesIdleAndPreview()
    {
        QFETCH(bool, preview);
        QFETCH(bool, debug);
        ApplicationController controller;
        disconnect(&controller.m_stateMachine, &ScreensaverStateMachine::activationRequested,
                   &controller, &ApplicationController::activate);
        QSignalSpy activation(&controller.m_stateMachine, &ScreensaverStateMachine::activationRequested);
        QSignalSpy teardown(&controller.m_overlays, &OverlayManager::teardownCompleted);
        controller.Preview();
        controller.finishActivation(); // Real QQuickView, on CTest's offscreen platform.
        QCOMPARE(controller.m_stateMachine.state(), ScreensaverStateMachine::State::Active);
        Q_EMIT controller.m_overlays.inputDetected();
        QCOMPARE(controller.m_stateMachine.state(), ScreensaverStateMachine::State::Dismissing);
        QCOMPARE(teardown.count(), 0);
        QVERIFY(!controller.m_overlays.show()); // Also guarded at the surface owner.
        // Shorten only the local timer, preserving the configured interval so
        // teardown's scheduling reconciliation leaves this deadline intact.
        controller.m_idleMonitor.m_freshIntervalTimer.start(100);
        // Offscreen has no KIdleTime poller. Seed only its token, then inject
        // backend signals through the production connection and timer gate.
        const int id = 42;
        controller.m_idleMonitor.m_timeoutId = id;
        Q_EMIT KIdleTime::instance()->timeoutReached(id, 100); // Already-idle/stale notification.
        Q_EMIT controller.m_idleMonitor.idleTimeoutReached(); // Dropped while Dismissing.
        if (preview) {
            if (debug) controller.PreviewDebug();
            else controller.Preview();
        }
        QCOMPARE(activation.count(), 1);
        QCoreApplication::sendPostedEvents(nullptr, QEvent::DeferredDelete);
        QCOMPARE(teardown.count(), 0); // Completion is after the destructor unwinds.
        QTRY_COMPARE(teardown.count(), 1);
        if (preview) {
            QCOMPARE(activation.count(), 2);
            QVERIFY(activation.last().first().toBool());
            QCOMPARE(controller.m_debugPreviewPending, debug);
        } else {
            QCOMPARE(controller.m_stateMachine.state(), ScreensaverStateMachine::State::Waiting);
            QCOMPARE(activation.count(), 1);
            Q_EMIT KIdleTime::instance()->timeoutReached(id, 100);
            QCOMPARE(activation.count(), 1);
            QTRY_COMPARE(activation.count(), 2);
            QVERIFY(!activation.last().first().toBool());
        }
        controller.m_idleMonitor.stop();
    }

    void freshIdleIntervalResetsOnResume()
    {
        bool watching = false;
        int armCount = 0;
        IdleMonitor monitor;
        observeResumeWatching(monitor, watching, armCount);
        QSignalSpy idle(&monitor, &IdleMonitor::idleTimeoutReached);
        monitor.start(160, true);
        const int id = 42;
        monitor.m_timeoutId = id; // No poller exists on the offscreen platform.
        Q_EMIT KIdleTime::instance()->timeoutReached(id, 160);
        QCOMPARE(idle.count(), 0);
        QTest::qWait(100);
        QVERIFY(watching);
        const int previousArmCount = armCount;
        backendResume(monitor);
        QVERIFY(!watching);
        QTRY_VERIFY(watching);
        QCOMPARE(armCount, previousArmCount + 1);
        Q_EMIT KIdleTime::instance()->timeoutReached(id, 160);
        QTest::qWait(100); // Original deadline passed, resumed deadline has not.
        QCOMPARE(idle.count(), 0);
        QTRY_COMPARE(idle.count(), 1);
        monitor.watchForResume(); // Cancel any old timeout and local timer.
        Q_EMIT KIdleTime::instance()->timeoutReached(id, 160);
        QCOMPARE(idle.count(), 1);
    }

    void pendingResumeRearmIsInvalidated_data()
    {
        QTest::addColumn<int>("restartInterval");
        QTest::addColumn<bool>("fresh");
        QTest::newRow("stop") << -1 << false;
        QTest::newRow("start-normal") << 2000 << false;
        QTest::newRow("start-fresh") << 2000 << true;
    }

    void pendingResumeRearmIsInvalidated()
    {
        QFETCH(int, restartInterval);
        QFETCH(bool, fresh);
        bool watching = false;
        int armCount = 0;
        IdleMonitor monitor;
        observeResumeWatching(monitor, watching, armCount);
        monitor.start(1000, true);
        QTRY_VERIFY(watching);
        const int previousArmCount = armCount;
        backendResume(monitor);
        QVERIFY(!watching);
        if (restartInterval < 0) monitor.stop();
        else monitor.start(restartInterval, fresh);
        QCoreApplication::sendPostedEvents(&monitor, QEvent::MetaCall);
        QCOMPARE(watching, fresh);
        // Only the newest fresh start may arm, never the pending old rearm.
        QCOMPARE(armCount, previousArmCount + (fresh ? 1 : 0));
        QCOMPARE(monitor.m_timeoutMilliseconds, restartInterval);
    }

    void teardownReconcilesIdleConfiguration_data()
    {
        QTest::addColumn<int>("newMinutes");
        QTest::newRow("changed-interval") << 3;
        QTest::newRow("unchanged-interval") << 1;
    }

    void teardownReconcilesIdleConfiguration()
    {
        QFETCH(int, newMinutes);
        ApplicationController controller;
        disconnect(&controller.m_stateMachine, &ScreensaverStateMachine::activationRequested,
                   &controller, &ApplicationController::activate);
        controller.m_configuration.setIdleMinutes(1);
        controller.Preview();
        controller.m_stateMachine.activationSucceeded();
        controller.requestDismissal("input");
        QCOMPARE(controller.m_stateMachine.state(), ScreensaverStateMachine::State::Dismissing);
        QCOMPARE(controller.m_idleMonitor.m_freshIntervalTimer.interval(), 60000);
        const auto timerId = controller.m_idleMonitor.m_freshIntervalTimer.id();
        controller.m_idleMonitor.m_timeoutId = 42;
        Q_EMIT KIdleTime::instance()->timeoutReached(42, 60000);
        controller.m_configuration.setIdleMinutes(newMinutes);
        controller.m_configuration.save();
        QCOMPARE(controller.m_idleMonitor.m_timeoutMilliseconds, 60000);
        Q_EMIT controller.m_overlays.teardownCompleted();
        QCOMPARE(controller.m_stateMachine.state(), ScreensaverStateMachine::State::Waiting);
        QCOMPARE(controller.m_idleMonitor.m_timeoutMilliseconds, newMinutes * 60000);
        QCOMPARE(controller.m_idleMonitor.m_freshIntervalTimer.interval(), newMinutes * 60000);
        QVERIFY(controller.m_idleMonitor.m_freshInterval);
        QVERIFY(controller.m_idleMonitor.m_freshIntervalTimer.isActive());
        QVERIFY(controller.m_requireFreshIdleInterval);
        if (newMinutes == 1) {
            QCOMPARE(controller.m_idleMonitor.m_freshIntervalTimer.id(), timerId);
            QVERIFY(controller.m_idleMonitor.m_backendIdle);
        } else {
            QVERIFY(controller.m_idleMonitor.m_freshIntervalTimer.id() != timerId);
            QVERIFY(!controller.m_idleMonitor.m_backendIdle);
        }
    }

    void freshIntervalAloneDoesNotActivate()
    {
        IdleMonitor monitor;
        QSignalSpy idle(&monitor, &IdleMonitor::idleTimeoutReached);
        monitor.start(40, true);
        const int id = 42;
        monitor.m_timeoutId = id; // No poller exists on the offscreen platform.
        QTest::qWait(80);
        QCOMPARE(idle.count(), 0);
        Q_EMIT KIdleTime::instance()->timeoutReached(id, 40);
        QCOMPARE(idle.count(), 1);
        monitor.stop();
        Q_EMIT KIdleTime::instance()->timeoutReached(id, 40);
        QCOMPARE(idle.count(), 1);
    }

    void mappingResumeDoesNotDismiss()
    {
        bool watching = false;
        int armCount = 0;
        ApplicationController controller;
        observeResumeWatching(controller.m_idleMonitor, watching, armCount);
        disconnect(&controller.m_stateMachine, &ScreensaverStateMachine::activationRequested,
                   &controller, &ApplicationController::activate);
        controller.Preview();
        controller.finishActivation();
        QVERIFY(controller.m_overlays.inputGraceActive());
        controller.m_idleMonitor.watchForResume();
        QTRY_VERIFY(watching);
        backendResume(controller.m_idleMonitor);
        QVERIFY(!watching);
        QCOMPARE(controller.m_stateMachine.state(), ScreensaverStateMachine::State::Active);
        QTRY_VERIFY(watching);
        // A later compositor-only resume still dismisses after mapping grace.
        QTRY_VERIFY(!controller.m_overlays.inputGraceActive());
        backendResume(controller.m_idleMonitor);
        QVERIFY(!watching);
        QCOMPARE(controller.m_stateMachine.state(), ScreensaverStateMachine::State::Dismissing);
        QTRY_COMPARE(controller.m_stateMachine.state(), ScreensaverStateMachine::State::Waiting);
        // Dismissal's start(..., true), called within the resume dispatch, also
        // arms after the backend cancellation and retains the fresh interval.
        QTRY_VERIFY(watching);
        QVERIFY(controller.m_idleMonitor.m_freshInterval);
    }

    void updateCenterReportsDestination_data()
    {
        QTest::addColumn<QString>("discover");
        QTest::addColumn<bool>("canOpenRelease");
        QTest::addColumn<QString>("expected");
        QTest::newRow("discover-started") << QStringLiteral("working") << true << QStringLiteral("discover");
        QTest::newRow("discover-missing") << QStringLiteral("missing") << true << QStringLiteral("releases");
        QTest::newRow("discover-start-failed") << QStringLiteral("broken") << true << QStringLiteral("releases");
        QTest::newRow("both-failed") << QStringLiteral("broken") << false << QStringLiteral("failed");
        QTest::newRow("release-failed-no-discover") << QStringLiteral("missing") << false << QStringLiteral("failed");
    }

    void updateCenterReportsDestination()
    {
        QFETCH(QString, discover);
        QFETCH(bool, canOpenRelease);
        QFETCH(QString, expected);
        QTemporaryDir directory;
        QVERIFY(directory.isValid());
        const QString executable = directory.filePath(QStringLiteral("plasma-discover"));
        if (discover == QStringLiteral("working")) {
            QVERIFY(QFile::link(QStringLiteral("/usr/bin/true"), executable));
        } else if (discover == QStringLiteral("broken")) {
            QFile file(executable);
            QVERIFY(file.open(QIODevice::WriteOnly));
            QVERIFY(file.write("invalid executable\n") > 0);
            file.close();
            QVERIFY(file.setPermissions(QFileDevice::ReadOwner | QFileDevice::WriteOwner | QFileDevice::ExeOwner));
        }
        const QByteArray originalPath = qgetenv("PATH");
        const auto restore = qScopeGuard([&] {
            qputenv("PATH", originalPath);
            QDesktopServices::unsetUrlHandler(QStringLiteral("https"));
        });
        qputenv("PATH", directory.path().toUtf8());
        m_openedRelease = QUrl();
        m_releaseCount = 0;
        // A registered handler prevents opening the user's browser. A missing
        // handler method makes openUrl fail deterministically as well.
        QDesktopServices::setUrlHandler(QStringLiteral("https"), this,
                                       canOpenRelease ? "captureReleasePage" : "missingReleaseHandler");
        ApplicationController controller;
        QCOMPARE(controller.openUpdateCenter(), expected);
        const bool openedRelease = expected == QStringLiteral("releases");
        QCOMPARE(m_releaseCount, openedRelease ? 1 : 0);
        if (openedRelease) {
            QCOMPARE(m_openedRelease, QUrl(QStringLiteral(
                "https://github.com/StantonMatt/plasma-wayland-screensaver/releases/latest")));
        }
    }

    void noUsableScreenReportsPreviewFailure()
    {
        QVERIFY(QGuiApplication::screens().isEmpty());
        ApplicationController controller;
        // Inhibitor acquisition is covered above. Isolate the next stage so the
        // real OverlayManager::show() exercises its empty-screen failure branch.
        disconnect(&controller.m_stateMachine, &ScreensaverStateMachine::activationRequested,
                   &controller, &ApplicationController::activate);
        QSignalSpy failed(&controller, &ApplicationController::previewFailed);
        controller.m_activationIsPreview = true;
        controller.m_stateMachine.previewRequested();
        controller.finishActivation();
        QCOMPARE(failed.count(), 1);
        QCOMPARE(failed.constFirst().constFirst().toString(), QStringLiteral("The display couldn't be prepared."));
        QVERIFY(!controller.screensaverActive());
        QTRY_COMPARE(controller.m_stateMachine.state(), ScreensaverStateMachine::State::Waiting);
        controller.m_stateMachine.idleTimeoutReached();
        controller.finishActivation();
        QCOMPARE(failed.count(), 1);
        QVERIFY(!controller.screensaverActive());
    }

    void exposesReactiveMonitorCount()
    {
        ApplicationController controller;
        QCOMPARE(controller.monitorCount(), QGuiApplication::screens().size());
        QSignalSpy changed(&controller, &ApplicationController::monitorCountChanged);
        // Exercise the application's real hotplug signal connections.
        Q_EMIT qGuiApp->screenAdded(QGuiApplication::primaryScreen());
        QCOMPARE(changed.count(), 1);
        Q_EMIT qGuiApp->screenRemoved(QGuiApplication::primaryScreen());
        QCOMPARE(changed.count(), 2);
    }

    void settingsWindowLoadsFromResources()
    {
        // Any warning from the window's own QML fails the test.
        static QStringList warnings;
        static QtMessageHandler previous = nullptr;
        warnings.clear();
        previous = qInstallMessageHandler([](QtMsgType type, const QMessageLogContext &context, const QString &message) {
            if (type != QtDebugMsg && type != QtInfoMsg && message.contains(QStringLiteral("qrc:/qml/")))
                warnings.append(message);
            if (previous) previous(type, context, message);
        });
        const auto restoreHandler = qScopeGuard([] { qInstallMessageHandler(previous); });

        ApplicationController controller;
        QFile license(QStringLiteral(":/LICENSE"));
        QVERIFY(license.open(QIODevice::ReadOnly));
        QCOMPARE(controller.applicationLicenseText(), QString::fromUtf8(license.readAll()));
        QVERIFY(controller.applicationLicenseText().contains(QStringLiteral("END OF TERMS AND CONDITIONS")));
        auto *config = controller.configuration();
        config->restoreDefaults();
        controller.ShowSettings();
        QVERIFY(controller.m_settingsEngine);
        QVERIFY(!controller.m_settingsEngine->rootObjects().isEmpty());
        auto *window = qobject_cast<QQuickWindow *>(controller.m_settingsEngine->rootObjects().constFirst());
        QVERIFY(window);
        QTRY_VERIFY(window->isVisible());
        QCOMPARE(window->property("currentPage").toString(), QStringLiteral("appearance"));

        for (const QString &page : {QStringLiteral("general"), QStringLiteral("about"), QStringLiteral("appearance")}) {
            QVERIFY(QMetaObject::invokeMethod(window, "showPage", Q_ARG(QVariant, page)));
            QCOMPARE(window->property("currentPage").toString(), page);
        }
        // Every animation's options, through the real Configuration profiles.
        for (const QString &module : {QStringLiteral("none"), QStringLiteral("orbs"), QStringLiteral("bounce"),
                 QStringLiteral("starfield"), QStringLiteral("matrix"), QStringLiteral("kaleidoscope"),
                 QStringLiteral("fireflies"), QStringLiteral("ribbons"), QStringLiteral("constellation"),
                 QStringLiteral("snakes"), QStringLiteral("aurora")}) {
            config->setVisualModule(module);
            QCoreApplication::processEvents();
        }

        // A real preview cannot start here (no session bus); the window says so.
        QVERIFY(QMetaObject::invokeMethod(window, "preview"));
        QTRY_COMPARE(window->property("previewError").toString(),
                     QStringLiteral("Couldn't start the preview. The display couldn't be kept awake."));

        // Closing only hides the window; the service keeps running.
        window->close();
        QTRY_VERIFY(!window->isVisible());
        QVERIFY(controller.m_settingsEngine);
        controller.ShowSettings();
        QTRY_VERIFY(window->isVisible());
        QVERIFY2(warnings.isEmpty(), qPrintable(warnings.join(QLatin1Char('\n'))));
    }
};

int main(int argc, char **argv)
{
    QTemporaryDir directory;
    if (!directory.isValid()) return 1;
    qputenv("XDG_CONFIG_HOME", directory.path().toUtf8());
    // No portal/PowerDevil or activation on the user's real session bus.
    qputenv("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent/pvs-controller-test-bus");
    QGuiApplication app(argc, argv);
    app.setQuitOnLastWindowClosed(false);
    ApplicationControllerTest test;
    return QTest::qExec(&test, argc, argv);
}
#include "test_applicationcontroller.moc"

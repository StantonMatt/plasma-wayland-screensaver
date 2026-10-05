// SPDX-License-Identifier: GPL-3.0-or-later
#include "applicationcontroller.h"
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
        controller.m_stateMachine.idleTimeoutReached();
        QTRY_VERIFY(!controller.screensaverActive());
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

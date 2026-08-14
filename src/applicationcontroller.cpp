// SPDX-License-Identifier: GPL-3.0-or-later
#include "applicationcontroller.h"

#include <QCoreApplication>
#include <QDebug>
#include <QDesktopServices>
#include <QGuiApplication>
#include <QProcess>
#include <QQmlApplicationEngine>
#include <QQuickWindow>
#include <QStandardPaths>
#include <QTimer>
#include <QUrl>
#include <QVariantMap>

ApplicationController::ApplicationController(QObject *parent)
    : QObject(parent)
    , m_overlays(&m_configuration, this)
{
    connect(&m_idleMonitor, &IdleMonitor::idleTimeoutReached,
            &m_stateMachine, &ScreensaverStateMachine::idleTimeoutReached);
    connect(&m_idleMonitor, &IdleMonitor::activityResumed, this, [this] {
        if (m_stateMachine.isActive()) {
            m_stateMachine.activityDetected();
        } else {
            scheduleIdleTimeout();
        }
    });
    connect(&m_overlays, &OverlayManager::inputDetected,
            &m_stateMachine, &ScreensaverStateMachine::activityDetected);
    connect(&m_inhibitor, &Inhibitor::acquired,
            this, &ApplicationController::finishActivation);
    connect(&m_inhibitor, &Inhibitor::failed,
            this, &ApplicationController::failActivation);
    connect(&m_stateMachine, &ScreensaverStateMachine::activationRequested,
            this, &ApplicationController::activate);
    connect(&m_stateMachine, &ScreensaverStateMachine::dismissalRequested,
            this, &ApplicationController::dismiss);
    connect(&m_stateMachine, &ScreensaverStateMachine::stateChanged,
            this, &ApplicationController::screensaverActiveChanged);
    connect(&m_configuration, &Configuration::saved,
            this, &ApplicationController::scheduleIdleTimeout);
    connect(qApp, &QCoreApplication::aboutToQuit, this, [this] {
        m_idleMonitor.stop();
        m_overlays.hide();
        m_inhibitor.release();
    });
}

ApplicationController::~ApplicationController() = default;

Configuration *ApplicationController::configuration()
{
    return &m_configuration;
}

bool ApplicationController::screensaverActive() const
{
    return m_stateMachine.isActive();
}

QString ApplicationController::applicationVersion() const
{
    return QCoreApplication::applicationVersion();
}

void ApplicationController::start()
{
    scheduleIdleTimeout();
}

void ApplicationController::ShowSettings()
{
    if (!m_settingsEngine) {
        m_settingsEngine = std::make_unique<QQmlApplicationEngine>();
        m_settingsEngine->setInitialProperties({
            {QStringLiteral("controller"), QVariant::fromValue(this)},
            {QStringLiteral("screensaverConfig"), QVariant::fromValue(&m_configuration)},
        });
        m_settingsEngine->load(QUrl(QStringLiteral("qrc:/qml/Settings.qml")));
        if (m_settingsEngine->rootObjects().isEmpty()) {
            qWarning() << "Could not load settings UI";
            m_settingsEngine.reset();
            return;
        }
    }

    if (auto *window = qobject_cast<QQuickWindow *>(m_settingsEngine->rootObjects().constFirst())) {
        window->show();
        window->raise();
        window->requestActivate();
    }
}

void ApplicationController::Preview()
{
    m_stateMachine.previewRequested();
}

void ApplicationController::Quit()
{
    m_stateMachine.stop();
    // Defer destruction when invoked by the QML button so the current signal
    // handler can unwind before its engine and window disappear. Destroying the
    // settings window also prevents its hide-on-close handler from vetoing quit.
    QTimer::singleShot(0, this, [this] {
        m_settingsEngine.reset();
        QCoreApplication::quit();
    });
}

void ApplicationController::saveSettings(const QVariantMap &settings)
{
    m_configuration.apply(settings);
    m_configuration.save();
}

bool ApplicationController::openUpdateCenter() const
{
    const QString discover = QStandardPaths::findExecutable(QStringLiteral("plasma-discover"));
    if (!discover.isEmpty()) {
        const QStringList arguments = {
            QStringLiteral("--mode"),
            QStringLiteral("Update"),
        };
        if (QProcess::startDetached(discover, arguments)) {
            return true;
        }
        qWarning() << "Could not start KDE Discover from" << discover;
    }

    const QUrl releasesUrl(QStringLiteral(
        "https://github.com/StantonMatt/plasma-wayland-screensaver/releases/latest"));
    if (QDesktopServices::openUrl(releasesUrl)) {
        return true;
    }

    qWarning() << "Could not open the Plasma Visual Screensaver update page";
    return false;
}

void ApplicationController::activate(bool preview)
{
    Q_UNUSED(preview)
    m_idleMonitor.watchForResume();
    m_inhibitor.acquire();
}

void ApplicationController::finishActivation()
{
    if (m_stateMachine.state() != ScreensaverStateMachine::State::Activating) {
        m_inhibitor.release();
        return;
    }
    if (!m_overlays.show()) {
        qWarning() << "No usable screen was available for the screensaver overlay";
        m_inhibitor.release();
        m_stateMachine.activationFailed();
        return;
    }
    m_stateMachine.activationSucceeded();
}

void ApplicationController::failActivation(const QString &error)
{
    if (m_stateMachine.state() != ScreensaverStateMachine::State::Activating) {
        return;
    }
    qWarning().noquote() << "Refusing to activate without a power/display inhibitor:" << error;
    m_stateMachine.activationFailed();
    // Do not spin while the current idle interval remains above the threshold.
    // KIdleTime's already-armed resume notification starts a fresh interval.
}

void ApplicationController::dismiss()
{
    m_overlays.hide();
    m_inhibitor.release();
    scheduleIdleTimeout();
}

void ApplicationController::scheduleIdleTimeout()
{
    if (m_stateMachine.state() != ScreensaverStateMachine::State::Waiting) {
        return;
    }
    constexpr int millisecondsPerMinute = 60 * 1000;
    m_idleMonitor.start(m_configuration.idleMinutes() * millisecondsPerMinute);
}

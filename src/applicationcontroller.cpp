// SPDX-License-Identifier: GPL-3.0-or-later
#include "applicationcontroller.h"

#include <QCoreApplication>
#include <QDebug>
#include <QDesktopServices>
#include <QFile>
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
    connect(qGuiApp, &QGuiApplication::screenAdded, this, &ApplicationController::monitorCountChanged);
    connect(qGuiApp, &QGuiApplication::screenRemoved, this, &ApplicationController::monitorCountChanged);
    connect(&m_idleMonitor, &IdleMonitor::idleTimeoutReached,
            &m_stateMachine, &ScreensaverStateMachine::idleTimeoutReached);
    connect(&m_idleMonitor, &IdleMonitor::activityResumed, this, [this] {
        if (m_stateMachine.isActive()) {
            m_waitForIdleResumeOnDismissal = false;
            m_stateMachine.activityDetected();
        } else {
            scheduleIdleTimeout();
        }
    });
    connect(&m_overlays, &OverlayManager::inputDetected, this, [this] {
        m_waitForIdleResumeOnDismissal = true;
        m_stateMachine.activityDetected();
    });
    connect(&m_overlays, &OverlayManager::overlayUnavailable, this, [this] {
        m_waitForIdleResumeOnDismissal = false;
        m_stateMachine.activityDetected();
    });
    connect(&m_inhibitor, &Inhibitor::acquired,
            this, &ApplicationController::finishActivation);
    connect(&m_inhibitor, &Inhibitor::failed, this, [this](const QString &error) {
        failActivation(error, tr("The display couldn't be kept awake."));
    });
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

int ApplicationController::monitorCount() const
{
    return QGuiApplication::screens().size();
}

QString ApplicationController::applicationVersion() const
{
    return QCoreApplication::applicationVersion();
}

QString ApplicationController::applicationLicenseText() const
{
    static const QString text = [] {
        QFile license(QStringLiteral(":/LICENSE"));
        if (!license.open(QIODevice::ReadOnly)) return QString();
        return QString::fromUtf8(license.readAll());
    }();
    return text;
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
    if (m_stateMachine.isActive()) {
        qWarning() << "Preview requested while the screensaver is starting or running";
        Q_EMIT previewFailed(tr("The screensaver is already running."));
        return;
    }
    m_debugPreviewPending = false;
    m_stateMachine.previewRequested();
}

void ApplicationController::PreviewDebug()
{
    if (m_stateMachine.isActive()) {
        qWarning() << "Debug preview requested while the screensaver is starting or running";
        Q_EMIT previewFailed(tr("The screensaver is already running."));
        return;
    }
    m_debugPreviewPending = true;
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

QString ApplicationController::openUpdateCenter() const
{
    const QString discover = QStandardPaths::findExecutable(QStringLiteral("plasma-discover"));
    if (!discover.isEmpty()) {
        const QStringList arguments = {
            QStringLiteral("--mode"),
            QStringLiteral("Update"),
        };
        if (QProcess::startDetached(discover, arguments)) {
            return QStringLiteral("discover");
        }
        qWarning() << "Could not start KDE Discover from" << discover;
    }

    const QUrl releasesUrl(QStringLiteral(
        "https://github.com/StantonMatt/plasma-wayland-screensaver/releases/latest"));
    if (QDesktopServices::openUrl(releasesUrl)) {
        return QStringLiteral("releases");
    }

    qWarning() << "Could not open the Plasma Visual Screensaver update page";
    return QStringLiteral("failed");
}

void ApplicationController::activate(bool preview)
{
    m_activationIsPreview = preview;
    m_overlays.setDeveloperMode(preview && m_debugPreviewPending);
    m_debugPreviewPending = false;
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
        m_inhibitor.release();
        failActivation(QStringLiteral("No usable screen was available for the screensaver overlay"),
                       tr("The display couldn't be prepared."));
        return;
    }
    m_activationIsPreview = false;
    m_stateMachine.activationSucceeded();
}

void ApplicationController::failActivation(const QString &error, const QString &userReason)
{
    if (m_stateMachine.state() != ScreensaverStateMachine::State::Activating) {
        return;
    }
    qWarning().noquote() << "Screensaver activation failed:" << error;
    const bool preview = m_activationIsPreview;
    m_activationIsPreview = false;
    m_stateMachine.activationFailed();
    if (preview) Q_EMIT previewFailed(userReason);
    // Do not spin while the current idle interval remains above the threshold.
    // KIdleTime's already-armed resume notification starts a fresh interval.
}

void ApplicationController::dismiss()
{
    m_activationIsPreview = false;
    m_overlays.hide();
    m_inhibitor.release();
    if (m_waitForIdleResumeOnDismissal) {
        m_waitForIdleResumeOnDismissal = false;
        // Qt can deliver the overlay input before KIdleTime's Wayland backend
        // observes it. Preserve the resume watch and let activityResumed arm
        // the next timeout after the compositor has reset its idle clock.
        m_idleMonitor.clearTimeoutWhileWaitingForResume();
    } else {
        scheduleIdleTimeout();
    }
}

void ApplicationController::scheduleIdleTimeout()
{
    if (m_stateMachine.state() != ScreensaverStateMachine::State::Waiting) {
        return;
    }
    constexpr int millisecondsPerMinute = 60 * 1000;
    m_idleMonitor.start(m_configuration.idleMinutes() * millisecondsPerMinute);
}

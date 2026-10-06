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
            // Surface mapping can produce a compositor resume without user input.
            if (m_overlays.inputGraceActive()) {
                m_idleMonitor.watchForResume();
                return;
            }
            requestDismissal("idle resume");
        } else {
            scheduleIdleTimeout();
        }
    });
    connect(&m_overlays, &OverlayManager::inputDetected, this, [this] {
        requestDismissal("input");
    });
    connect(&m_overlays, &OverlayManager::overlayUnavailable, this, [this] {
        requestDismissal("output change");
    });
    connect(&m_overlays, &OverlayManager::teardownCompleted, this, [this] {
        m_stateMachine.teardownCompleted();
        // Reconcile settings saved during Dismissing without restarting an
        // unchanged interval, then reconsider any idle notification dropped
        // during a long teardown. Preview takes precedence.
        if (m_stateMachine.state() == ScreensaverStateMachine::State::Waiting && !m_quitting) {
            scheduleIdleTimeout();
            m_idleMonitor.checkIdleTimeout();
        }
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
        m_quitting = true;
        requestDismissal("quit");
        m_stateMachine.stop();
        m_idleMonitor.stop();
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
    if (m_quitting) return;
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
    if (m_quitting) return;
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
    m_quitting = true;
    requestDismissal("quit");
    m_stateMachine.stop();
    m_idleMonitor.stop();
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
    qInfo().noquote() << QStringLiteral("Activating screensaver (source: %1)")
                            .arg(preview ? QStringLiteral("preview") : QStringLiteral("idle"));
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
    m_dismissalReason = "activation failure";
    m_stateMachine.activationFailed();
    if (preview) Q_EMIT previewFailed(userReason);
}

void ApplicationController::requestDismissal(const char *reason)
{
    if (!m_stateMachine.isActive()) return;
    m_dismissalReason = reason;
    m_stateMachine.activityDetected();
}

void ApplicationController::dismiss()
{
    qInfo().noquote() << QStringLiteral("Dismissing screensaver (reason: %1)")
                            .arg(QString::fromLatin1(m_dismissalReason));
    m_activationIsPreview = false;
    m_requireFreshIdleInterval = true;
    if (!m_quitting) {
        constexpr int millisecondsPerMinute = 60 * 1000;
        m_idleMonitor.start(m_configuration.idleMinutes() * millisecondsPerMinute, true);
    }
    m_inhibitor.release();
    m_overlays.hide();
}

void ApplicationController::scheduleIdleTimeout()
{
    if (m_quitting || m_stateMachine.state() != ScreensaverStateMachine::State::Waiting) {
        return;
    }
    constexpr int millisecondsPerMinute = 60 * 1000;
    m_idleMonitor.start(m_configuration.idleMinutes() * millisecondsPerMinute, m_requireFreshIdleInterval);
}

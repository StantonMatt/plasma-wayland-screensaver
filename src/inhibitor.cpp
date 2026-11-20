// SPDX-License-Identifier: GPL-3.0-or-later
#include "inhibitor.h"

#include <QDBusConnection>
#include <QDBusInterface>
#include <QDBusReply>
#include <QDebug>
#include <QRandomGenerator>
#include <QVariantMap>

namespace {
constexpr auto portalService = "org.freedesktop.portal.Desktop";
constexpr auto portalPath = "/org/freedesktop/portal/desktop";
constexpr auto portalInhibitInterface = "org.freedesktop.portal.Inhibit";
constexpr auto requestInterface = "org.freedesktop.portal.Request";
constexpr auto powerService = "org.freedesktop.PowerManagement";
constexpr auto powerPath = "/org/freedesktop/PowerManagement/Inhibit";
constexpr auto powerInterface = "org.freedesktop.PowerManagement.Inhibit";
constexpr auto screenSaverService = "org.freedesktop.ScreenSaver";
// The standard ScreenSaver API path, also exported by KScreenLocker.
constexpr auto screenSaverPath = "/ScreenSaver";
constexpr auto screenSaverInterface = "org.freedesktop.ScreenSaver";
}

Inhibitor::Inhibitor(QObject *parent)
    : QObject(parent)
{
}

Inhibitor::~Inhibitor()
{
    release();
}

void Inhibitor::acquire()
{
    if (isHeld()) {
        Q_EMIT acquired();
        return;
    }
    if (m_acquiring) {
        return;
    }
    m_lastError.clear();
    m_acquiring = true;
    if (!beginPortalAcquire()) {
        finishWithPowerManagementFallback();
    }
}

bool Inhibitor::beginPortalAcquire()
{
    QDBusInterface portal(QString::fromLatin1(portalService), QString::fromLatin1(portalPath),
                          QString::fromLatin1(portalInhibitInterface), QDBusConnection::sessionBus());
    if (!portal.isValid()) {
        m_lastError = portal.lastError().message();
        return false;
    }

    QVariantMap options;
    const QString token = QStringLiteral("plasmavisualscreensaver_%1")
                              .arg(QRandomGenerator::global()->generate());
    options.insert(QStringLiteral("reason"), QStringLiteral("Visual screensaver is active"));
    options.insert(QStringLiteral("handle_token"), token);

    QString sender = QDBusConnection::sessionBus().baseService();
    sender.remove(QLatin1Char(':'));
    sender.replace(QLatin1Char('.'), QLatin1Char('_'));
    const QString predictedPath = QStringLiteral("/org/freedesktop/portal/desktop/request/%1/%2")
                                      .arg(sender, token);
    if (!connectPortalResponse(predictedPath)) {
        m_lastError = QStringLiteral("Could not subscribe to the portal inhibition response");
        return false;
    }
    m_portalHandle = QDBusObjectPath(predictedPath);

    // Portal flags: 4 = suspend, 8 = idle (which includes display power management).
    const QDBusReply<QDBusObjectPath> reply = portal.call(QStringLiteral("Inhibit"), QString(), uint(4 | 8), options);
    if (!reply.isValid() || reply.value().path().isEmpty()) {
        disconnectPortalResponse();
        m_portalHandle = QDBusObjectPath();
        m_lastError = reply.error().message();
        return false;
    }

    // A fast portal may deliver Response while the blocking method call is
    // unwinding. Preserve only the handle for a confirmed portal acquisition;
    // a fallback backend must not accidentally retain the request object.
    if (!m_acquiring) {
        if (m_backend == Backend::Portal) {
            m_portalHandle = reply.value();
        } else {
            QDBusInterface request(QString::fromLatin1(portalService), reply.value().path(),
                                   QString::fromLatin1(requestInterface), QDBusConnection::sessionBus());
            request.call(QDBus::NoBlock, QStringLiteral("Close"));
            m_portalHandle = QDBusObjectPath();
        }
        return true;
    }
    m_portalHandle = reply.value();
    if (m_portalHandle.path() != predictedPath) {
        disconnectPortalResponse();
        if (!connectPortalResponse(m_portalHandle.path())) {
            m_lastError = QStringLiteral("Could not subscribe to the returned portal inhibition request");
            QDBusInterface request(QString::fromLatin1(portalService), m_portalHandle.path(),
                                   QString::fromLatin1(requestInterface), QDBusConnection::sessionBus());
            request.call(QDBus::NoBlock, QStringLiteral("Close"));
            m_portalHandle = QDBusObjectPath();
            return false;
        }
    }
    return true;
}

bool Inhibitor::connectPortalResponse(const QString &path)
{
    if (!QDBusConnection::sessionBus().connect(QString::fromLatin1(portalService), path,
                                               QString::fromLatin1(requestInterface),
                                               QStringLiteral("Response"), this,
                                               SLOT(onPortalResponse(uint,QVariantMap)))) {
        return false;
    }
    m_portalResponsePath = path;
    return true;
}

void Inhibitor::disconnectPortalResponse()
{
    if (m_portalResponsePath.isEmpty()) {
        return;
    }
    QDBusConnection::sessionBus().disconnect(QString::fromLatin1(portalService), m_portalResponsePath,
                                              QString::fromLatin1(requestInterface),
                                              QStringLiteral("Response"), this,
                                              SLOT(onPortalResponse(uint,QVariantMap)));
    m_portalResponsePath.clear();
}

void Inhibitor::onPortalResponse(uint response, const QVariantMap &results)
{
    Q_UNUSED(results)
    if (!m_acquiring) {
        return;
    }
    disconnectPortalResponse();
    if (response == 0) {
        m_acquiring = false;
        m_backend = Backend::Portal;
        qInfo() << "Acquired XDG portal idle/suspend inhibition";
        Q_EMIT acquired();
        return;
    }

    m_lastError = QStringLiteral("Portal inhibition request failed with response %1").arg(response);
    m_portalHandle = QDBusObjectPath();
    finishWithPowerManagementFallback();
}

void Inhibitor::finishWithPowerManagementFallback()
{
    if (!m_acquiring) {
        return;
    }
    if (!acquirePowerManagement()) {
        qInfo() << "Could not acquire optional power-management inhibition fallback:" << m_lastError;
    }
    if (acquireScreenSaver()) {
        m_backend = Backend::Fallback;
        m_acquiring = false;
        Q_EMIT acquired();
        return;
    }
    // Display/locking inhibition is required. Undo any suspend inhibition
    // already acquired before refusing activation.
    release();
    Q_EMIT failed(m_lastError);
}

bool Inhibitor::acquirePowerManagement()
{
    QDBusInterface power(QString::fromLatin1(powerService), QString::fromLatin1(powerPath),
                         QString::fromLatin1(powerInterface), QDBusConnection::sessionBus());
    if (!power.isValid()) {
        const QString fallbackError = QStringLiteral("PowerManagement inhibition failed: %1").arg(power.lastError().message());
        m_lastError = m_lastError.isEmpty() ? fallbackError : m_lastError + QStringLiteral("; ") + fallbackError;
        return false;
    }
    const QDBusReply<uint> reply = power.call(QStringLiteral("Inhibit"),
                                              QStringLiteral("plasma-visual-screensaver"),
                                              QStringLiteral("Visual screensaver is active"));
    if (!reply.isValid()) {
        const QString fallbackError = QStringLiteral("PowerManagement inhibition failed: %1").arg(reply.error().message());
        m_lastError = m_lastError.isEmpty() ? fallbackError : m_lastError + QStringLiteral("; ") + fallbackError;
        return false;
    }
    m_powerCookie = reply.value();
    m_powerHeld = true;
    qInfo() << "Acquired PowerDevil power-management inhibition fallback";
    return true;
}

bool Inhibitor::acquireScreenSaver()
{
    QDBusInterface screenSaver(QString::fromLatin1(screenSaverService), QString::fromLatin1(screenSaverPath),
                               QString::fromLatin1(screenSaverInterface), QDBusConnection::sessionBus());
    if (!screenSaver.isValid()) {
        const QString fallbackError = QStringLiteral("ScreenSaver inhibition failed: %1").arg(screenSaver.lastError().message());
        m_lastError = m_lastError.isEmpty() ? fallbackError : m_lastError + QStringLiteral("; ") + fallbackError;
        return false;
    }
    const QDBusReply<uint> reply = screenSaver.call(QStringLiteral("Inhibit"),
                                                   QStringLiteral("plasma-visual-screensaver"),
                                                   QStringLiteral("Visual screensaver is active"));
    if (!reply.isValid()) {
        const QString fallbackError = QStringLiteral("ScreenSaver inhibition failed: %1").arg(reply.error().message());
        m_lastError = m_lastError.isEmpty() ? fallbackError : m_lastError + QStringLiteral("; ") + fallbackError;
        return false;
    }
    m_screenSaverCookie = reply.value();
    m_screenSaverHeld = true;
    qInfo() << "Acquired ScreenSaver idle/display/locking inhibition fallback";
    return true;
}

void Inhibitor::release()
{
    const Backend releasedBackend = m_backend;
    const bool cancelledAcquire = m_acquiring;
    m_acquiring = false;
    disconnectPortalResponse();
    if (m_backend == Backend::Portal) {
        QDBusInterface request(QString::fromLatin1(portalService), m_portalHandle.path(),
                               QString::fromLatin1(requestInterface), QDBusConnection::sessionBus());
        request.call(QDBus::NoBlock, QStringLiteral("Close"));
    } else if (cancelledAcquire && !m_portalHandle.path().isEmpty()) {
        QDBusInterface request(QString::fromLatin1(portalService), m_portalHandle.path(),
                               QString::fromLatin1(requestInterface), QDBusConnection::sessionBus());
        request.call(QDBus::NoBlock, QStringLiteral("Close"));
    }
    if (m_screenSaverHeld) {
        QDBusInterface screenSaver(QString::fromLatin1(screenSaverService), QString::fromLatin1(screenSaverPath),
                                   QString::fromLatin1(screenSaverInterface), QDBusConnection::sessionBus());
        screenSaver.call(QDBus::NoBlock, QStringLiteral("UnInhibit"), m_screenSaverCookie);
    }
    if (m_powerHeld) {
        QDBusInterface power(QString::fromLatin1(powerService), QString::fromLatin1(powerPath),
                             QString::fromLatin1(powerInterface), QDBusConnection::sessionBus());
        power.call(QDBus::NoBlock, QStringLiteral("UnInhibit"), m_powerCookie);
    }
    m_backend = Backend::None;
    m_portalHandle = QDBusObjectPath();
    m_powerCookie = 0;
    m_screenSaverCookie = 0;
    m_powerHeld = false;
    m_screenSaverHeld = false;
    if (releasedBackend != Backend::None) {
        qInfo() << "Released screensaver power/display inhibition";
    }
}

bool Inhibitor::isHeld() const
{
    return m_backend != Backend::None;
}

QString Inhibitor::lastError() const
{
    return m_lastError;
}

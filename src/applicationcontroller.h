// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once

#include <QObject>
#include <QNetworkAccessManager>
#include <QPointer>
#include <QString>
#include <QUrl>
#include <memory>

#include "configuration.h"
#include "idlemonitor.h"
#include "inhibitor.h"
#include "overlaymanager.h"
#include "screensaverstatemachine.h"

class QQmlApplicationEngine;
class QNetworkReply;

class ApplicationController final : public QObject
{
    Q_OBJECT
    friend class ApplicationControllerTest;
    Q_CLASSINFO("D-Bus Interface", "org.kde.PlasmaVisualScreensaver")
    Q_PROPERTY(Configuration *configuration READ configuration CONSTANT)
    Q_PROPERTY(bool screensaverActive READ screensaverActive NOTIFY screensaverActiveChanged)
    Q_PROPERTY(int monitorCount READ monitorCount NOTIFY monitorCountChanged)
    Q_PROPERTY(QString applicationVersion READ applicationVersion CONSTANT)
    Q_PROPERTY(QString applicationLicenseText READ applicationLicenseText CONSTANT)

public:
    explicit ApplicationController(QObject *parent = nullptr);
    ~ApplicationController() override;

    Configuration *configuration();
    bool screensaverActive() const;
    int monitorCount() const;
    QString applicationVersion() const;
    QString applicationLicenseText() const;
    void start();

public Q_SLOTS:
    Q_SCRIPTABLE void ShowSettings();
    Q_SCRIPTABLE void Preview();
    Q_SCRIPTABLE void PreviewDebug();
    Q_SCRIPTABLE void Quit();
    // Returns "discover" or "checking"; asynchronous lookup emits its result.
    Q_INVOKABLE QString openUpdateCenter();

Q_SIGNALS:
    void screensaverActiveChanged();
    void monitorCountChanged();
    void previewFailed(const QString &reason);
    void updateCenterFinished(const QString &destination);

private Q_SLOTS:
    void activate(bool preview);
    void finishActivation();
    void failActivation(const QString &error, const QString &userReason);
    void dismiss();
    void scheduleIdleTimeout();

private:
    void requestDismissal(const char *reason);

    QNetworkAccessManager m_updateNetwork;
    QPointer<QNetworkReply> m_updateReply;
    QUrl m_updateApiUrl;
    Configuration m_configuration;
    IdleMonitor m_idleMonitor;
    Inhibitor m_inhibitor;
    OverlayManager m_overlays;
    ScreensaverStateMachine m_stateMachine;
    std::unique_ptr<QQmlApplicationEngine> m_settingsEngine;
    bool m_activationIsPreview = false;
    bool m_debugPreviewPending = false;
    bool m_requireFreshIdleInterval = false;
    bool m_quitting = false;
    const char *m_dismissalReason = "activity";
};

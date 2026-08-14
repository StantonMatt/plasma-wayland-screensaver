// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once

#include <QObject>
#include <QString>
#include <QVariantMap>
#include <memory>

#include "configuration.h"
#include "idlemonitor.h"
#include "inhibitor.h"
#include "overlaymanager.h"
#include "screensaverstatemachine.h"

class QQmlApplicationEngine;

class ApplicationController final : public QObject
{
    Q_OBJECT
    Q_CLASSINFO("D-Bus Interface", "org.kde.PlasmaVisualScreensaver")
    Q_PROPERTY(Configuration *configuration READ configuration CONSTANT)
    Q_PROPERTY(bool screensaverActive READ screensaverActive NOTIFY screensaverActiveChanged)
    Q_PROPERTY(QString applicationVersion READ applicationVersion CONSTANT)

public:
    explicit ApplicationController(QObject *parent = nullptr);
    ~ApplicationController() override;

    Configuration *configuration();
    bool screensaverActive() const;
    QString applicationVersion() const;
    void start();

public Q_SLOTS:
    Q_SCRIPTABLE void ShowSettings();
    Q_SCRIPTABLE void Preview();
    Q_SCRIPTABLE void Quit();
    Q_INVOKABLE void saveSettings(const QVariantMap &settings);
    Q_INVOKABLE bool openUpdateCenter() const;

Q_SIGNALS:
    void screensaverActiveChanged();

private Q_SLOTS:
    void activate(bool preview);
    void finishActivation();
    void failActivation(const QString &error);
    void dismiss();
    void scheduleIdleTimeout();

private:
    Configuration m_configuration;
    IdleMonitor m_idleMonitor;
    Inhibitor m_inhibitor;
    OverlayManager m_overlays;
    ScreensaverStateMachine m_stateMachine;
    std::unique_ptr<QQmlApplicationEngine> m_settingsEngine;
};

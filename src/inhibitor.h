// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once

#include <QObject>
#include <QDBusObjectPath>
#include <QVariantMap>

class Inhibitor final : public QObject
{
    Q_OBJECT

public:
    explicit Inhibitor(QObject *parent = nullptr);
    ~Inhibitor() override;

    void acquire();
    void release();
    bool isHeld() const;
    QString lastError() const;

Q_SIGNALS:
    void acquired();
    void failed(const QString &error);

private Q_SLOTS:
    void onPortalResponse(uint response, const QVariantMap &results);

private:
    enum class Backend { None, Portal, PowerManagement };
    bool beginPortalAcquire();
    bool acquirePowerManagement();
    bool connectPortalResponse(const QString &path);
    void disconnectPortalResponse();
    void finishWithPowerManagementFallback();

    Backend m_backend = Backend::None;
    QDBusObjectPath m_portalHandle;
    QString m_portalResponsePath;
    uint m_powerCookie = 0;
    QString m_lastError;
    bool m_acquiring = false;
};

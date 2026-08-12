// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once

#include <QObject>
#include <QString>
#include <memory>

class KConfig;

class Configuration final : public QObject
{
    Q_OBJECT
    Q_PROPERTY(int idleMinutes READ idleMinutes WRITE setIdleMinutes NOTIFY changed)
    Q_PROPERTY(QString visualModule READ visualModule WRITE setVisualModule NOTIFY changed)
    Q_PROPERTY(QString backgroundStyle READ backgroundStyle WRITE setBackgroundStyle NOTIFY changed)
    Q_PROPERTY(bool showClock READ showClock WRITE setShowClock NOTIFY changed)
    Q_PROPERTY(QString clockMovement READ clockMovement WRITE setClockMovement NOTIFY changed)
    Q_PROPERTY(QString clockSpeed READ clockSpeed WRITE setClockSpeed NOTIFY changed)
    Q_PROPERTY(int frameRate READ frameRate WRITE setFrameRate NOTIFY changed)
    Q_PROPERTY(bool reducedMotion READ reducedMotion WRITE setReducedMotion NOTIFY changed)
    Q_PROPERTY(QString monitorBehavior READ monitorBehavior WRITE setMonitorBehavior NOTIFY changed)
    Q_PROPERTY(bool coverPanels READ coverPanels WRITE setCoverPanels NOTIFY changed)

public:
    explicit Configuration(const QString &filePath = {}, QObject *parent = nullptr);
    ~Configuration() override;

    int idleMinutes() const;
    QString visualModule() const;
    QString backgroundStyle() const;
    bool showClock() const;
    QString clockMovement() const;
    QString clockSpeed() const;
    int frameRate() const;
    bool reducedMotion() const;
    QString monitorBehavior() const;
    bool coverPanels() const;

    void setIdleMinutes(int value);
    void setVisualModule(const QString &value);
    void setBackgroundStyle(const QString &value);
    void setShowClock(bool value);
    void setClockMovement(const QString &value);
    void setClockSpeed(const QString &value);
    void setFrameRate(int value);
    void setReducedMotion(bool value);
    void setMonitorBehavior(const QString &value);
    void setCoverPanels(bool value);

    Q_INVOKABLE void reload();
    Q_INVOKABLE void save();
    Q_INVOKABLE void restoreDefaults();

Q_SIGNALS:
    void changed();
    void saved();

private:
    void assignDefaults();
    template<typename T> void update(T &member, const T &value);

    std::unique_ptr<KConfig> m_config;
    int m_idleMinutes = 10;
    QString m_visualModule = QStringLiteral("aurora");
    QString m_backgroundStyle = QStringLiteral("midnight");
    bool m_showClock = true;
    QString m_clockMovement = QStringLiteral("bounce");
    QString m_clockSpeed = QStringLiteral("normal");
    int m_frameRate = 30;
    bool m_reducedMotion = false;
    QString m_monitorBehavior = QStringLiteral("independent");
    bool m_coverPanels = true;
};

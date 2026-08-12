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
    Q_PROPERTY(int animationSpeed READ animationSpeed WRITE setAnimationSpeed NOTIFY changed)
    Q_PROPERTY(int animationDensity READ animationDensity WRITE setAnimationDensity NOTIFY changed)
    Q_PROPERTY(int animationScale READ animationScale WRITE setAnimationScale NOTIFY changed)
    Q_PROPERTY(QString animationPalette READ animationPalette WRITE setAnimationPalette NOTIFY changed)
    Q_PROPERTY(int trailAmount READ trailAmount WRITE setTrailAmount NOTIFY changed)
    Q_PROPERTY(int ballCount READ ballCount WRITE setBallCount NOTIFY changed)
    Q_PROPERTY(int ballGravity READ ballGravity WRITE setBallGravity NOTIFY changed)
    Q_PROPERTY(int ballElasticity READ ballElasticity WRITE setBallElasticity NOTIFY changed)
    Q_PROPERTY(bool ballCollisions READ ballCollisions WRITE setBallCollisions NOTIFY changed)
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
    int animationSpeed() const;
    int animationDensity() const;
    int animationScale() const;
    QString animationPalette() const;
    int trailAmount() const;
    int ballCount() const;
    int ballGravity() const;
    int ballElasticity() const;
    bool ballCollisions() const;
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
    void setAnimationSpeed(int value);
    void setAnimationDensity(int value);
    void setAnimationScale(int value);
    void setAnimationPalette(const QString &value);
    void setTrailAmount(int value);
    void setBallCount(int value);
    void setBallGravity(int value);
    void setBallElasticity(int value);
    void setBallCollisions(bool value);
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
    int m_animationSpeed = 100;
    int m_animationDensity = 50;
    int m_animationScale = 100;
    QString m_animationPalette = QStringLiteral("ocean");
    int m_trailAmount = 35;
    int m_ballCount = 5;
    int m_ballGravity = 35;
    int m_ballElasticity = 92;
    bool m_ballCollisions = true;
    bool m_showClock = true;
    QString m_clockMovement = QStringLiteral("bounce");
    QString m_clockSpeed = QStringLiteral("normal");
    int m_frameRate = 30;
    bool m_reducedMotion = false;
    QString m_monitorBehavior = QStringLiteral("independent");
    bool m_coverPanels = true;
};

// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once

#include <QObject>
#include <QMap>
#include <QString>
#include <QVariantMap>
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
    Q_PROPERTY(int snakeAggression READ snakeAggression WRITE setSnakeAggression NOTIFY changed)
    Q_PROPERTY(int snakeIntelligence READ snakeIntelligence WRITE setSnakeIntelligence NOTIFY changed)
    Q_PROPERTY(bool snakeSelfCollisions READ snakeSelfCollisions WRITE setSnakeSelfCollisions NOTIFY changed)
    Q_PROPERTY(bool snakeLengthLimit READ snakeLengthLimit WRITE setSnakeLengthLimit NOTIFY changed)
    Q_PROPERTY(bool snakePowerUps READ snakePowerUps WRITE setSnakePowerUps NOTIFY changed)
    Q_PROPERTY(bool snakeStorePowerUps READ snakeStorePowerUps WRITE setSnakeStorePowerUps NOTIFY changed)
    Q_PROPERTY(bool snakeWorldEvents READ snakeWorldEvents WRITE setSnakeWorldEvents NOTIFY changed)
    Q_PROPERTY(bool snakeDeadlyWalls READ snakeDeadlyWalls WRITE setSnakeDeadlyWalls NOTIFY changed)
    // 0 Off, 1 Centered, 2 Slowly, 3 Drifting, 4 Quickly.
    Q_PROPERTY(int clockMode READ clockMode WRITE setClockMode NOTIFY changed)
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
    int snakeIntelligence() const;
    int snakeAggression() const;
    bool snakeSelfCollisions() const;
    bool snakeDeadlyWalls() const;
    bool snakeLengthLimit() const;
    bool snakePowerUps() const;
    bool snakeStorePowerUps() const;
    bool snakeWorldEvents() const;
    int clockMode() const;
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
    void setSnakeIntelligence(int value);
    void setSnakeAggression(int value);
    void setSnakeSelfCollisions(bool value);
    void setSnakeDeadlyWalls(bool value);
    void setSnakeLengthLimit(bool value);
    void setSnakePowerUps(bool value);
    void setSnakeStorePowerUps(bool value);
    void setSnakeWorldEvents(bool value);
    void setClockMode(int mode);
    void setShowClock(bool value);
    void setClockMovement(const QString &value);
    void setClockSpeed(const QString &value);
    void setFrameRate(int value);
    void setReducedMotion(bool value);
    void setMonitorBehavior(const QString &value);
    void setCoverPanels(bool value);
    Q_INVOKABLE void apply(const QVariantMap &settings);
    // page is "appearance", "general", or empty for all settings. Appearance
    // maps include every animation profile; apply(snapshot(page)) restores Undo
    // atomically, including the selected module. Unknown pages return empty maps.
    Q_INVOKABLE QVariantMap defaults(const QString &page = {}) const;
    Q_INVOKABLE QVariantMap snapshot(const QString &page = {}) const;

    Q_INVOKABLE void reload();
    Q_INVOKABLE void save();
    Q_INVOKABLE void restoreDefaults(const QString &page = {});

Q_SIGNALS:
    void changed();
    void saved();

private:
    void beginUpdate();
    void endUpdate();
    void assignDefaults();
    QVariantMap activeAnimationSettings() const;
    void applyAnimationSettings(const QVariantMap &settings);
    template<typename T> void update(T &member, const T &value);

    std::unique_ptr<KConfig> m_config;
    // Profiles are touched only by config edits/load/save. Render/tick getters
    // keep reading the scalar active-profile fields below without map lookups.
    QMap<QString, QVariantMap> m_animationSettings;
    int m_updateDepth = {};
    bool m_changedPending = {};
    int m_idleMinutes = {};
    QString m_visualModule;
    QString m_backgroundStyle;
    int m_animationSpeed = {};
    int m_animationDensity = {};
    int m_animationScale = {};
    QString m_animationPalette;
    int m_trailAmount = {};
    int m_ballCount = {};
    int m_ballGravity = {};
    int m_ballElasticity = {};
    bool m_ballCollisions = {};
    int m_snakeIntelligence = {};
    int m_snakeAggression = {};
    bool m_snakeSelfCollisions = {};
    bool m_snakeDeadlyWalls = {};
    bool m_snakeLengthLimit = {};
    bool m_snakePowerUps = {};
    bool m_snakeStorePowerUps = {};
    bool m_snakeWorldEvents = {};
    bool m_showClock = {};
    QString m_clockMovement;
    QString m_clockSpeed;
    int m_frameRate = {};
    bool m_reducedMotion = {};
    QString m_monitorBehavior;
    bool m_coverPanels = {};
};

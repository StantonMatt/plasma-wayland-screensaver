// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once

#include "animationstate.h"

#include <QObject>
#include <QHash>
#include <QPointer>
#include <QtGlobal>
#include <memory>

class Configuration;
class QQuickView;
class QScreen;
class PresentationClock;
class SnakeRenderer;

class OverlayManager final : public QObject
{
    Q_OBJECT

public:
    explicit OverlayManager(Configuration *configuration, QObject *parent = nullptr);
    ~OverlayManager() override;

    bool show();
    void hide();
    bool isVisible() const;
    void setDeveloperMode(bool enabled);

Q_SIGNALS:
    void inputDetected();

protected:
    bool eventFilter(QObject *watched, QEvent *event) override;

private:
    bool addScreen(QScreen *screen);
    void removeScreen(QScreen *screen);
    void updateAllViewGeometry();
    void updateViewGeometry(QScreen *screen);
    void updateAnimationState();
    void updatePresentationClocks();
    void configureSnakeRenderSharing();
    bool isDismissEvent(const QEvent *event) const;

    Configuration *m_configuration;
    AnimationState m_animationState;
    QHash<QScreen *, QQuickView *> m_views;
    QHash<QScreen *, PresentationClock *> m_presentationClocks;
    QHash<QScreen *, SnakeRenderer *> m_snakeRenderers;
    QPointer<SnakeRenderer> m_snakeSimulationDriver;
    QScreen *m_animationDriverScreen = nullptr;
    qint64 m_animationEpochMs = 0;
    bool m_visible = false;
    bool m_sharedAnimationActive = false;
    bool m_developerMode = false;
};

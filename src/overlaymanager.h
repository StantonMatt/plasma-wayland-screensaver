// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once

#include "animationstate.h"

#include <QObject>
#include <QElapsedTimer>
#include <QSet>
#include <QHash>
#include <QPointer>
#include <QRect>
#include <QtGlobal>
#include <memory>

class Configuration;
class QQuickView;
class QScreen;
class PresentationClock;
class SnakeRenderer;
class SnakeSimulation;

class OverlayManager final : public QObject
{
    Q_OBJECT
    friend class OverlaySnakesTest;

public:
    explicit OverlayManager(Configuration *configuration, QObject *parent = nullptr);
    ~OverlayManager() override;

    bool show();
    void hide();
    bool isVisible() const;
    bool inputGraceActive() const;
    void setDeveloperMode(bool enabled);

Q_SIGNALS:
    void inputDetected();
    void overlayUnavailable();
    void teardownCompleted();

protected:
    bool eventFilter(QObject *watched, QEvent *event) override;

private:
    // GUI-thread owned. PresentationClock queues swap feedback before emitting
    // ticks; window geometry, screen hotplug and QObject destruction are GUI
    // events. No scene-graph/render callback may access these containers.
    bool addScreen(QScreen *screen);
    void removeScreen(QScreen *screen);
    void updateAllViewGeometry();
    void updateViewGeometry(QScreen *screen);
    void updateAnimationState();
    void updatePresentationClocks();
    void configureSnakeRenderSharing();
    void advanceSnakeSimulation(QScreen *screen, qint64 presentationNanoseconds);
    void retireView(QQuickView *view);
    void reclaimReleasedMemory();
    void finishTeardown();
    bool isDismissEvent(const QEvent *event) const;

    Configuration *m_configuration;
    AnimationState m_animationState;
    QHash<QScreen *, QQuickView *> m_views;
    QHash<QScreen *, QRect> m_screenGeometries;
    QHash<QScreen *, PresentationClock *> m_presentationClocks;
    QHash<QScreen *, SnakeRenderer *> m_snakeRenderers;
    QHash<QScreen *, SnakeSimulation *> m_snakeSimulations;
    std::unique_ptr<SnakeSimulation> m_sharedSnakeSimulation;
    QScreen *m_snakeArenaScreen = nullptr;
    QString m_snakeBehavior;
    QScreen *m_animationDriverScreen = nullptr;
    QScreen *m_ballArenaScreen = nullptr;
    qint64 m_animationEpochMs = 0;
    int m_pendingViewDeletions = 0;
    QElapsedTimer m_inputGraceTimer;
    QSet<QQuickView *> m_mappingViews;
    bool m_teardownPending = false;
    bool m_visible = false;
    bool m_sharedAnimationActive = false;
    bool m_developerMode = false;
};

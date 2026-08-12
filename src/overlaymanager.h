// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once

#include "animationstate.h"

#include <QObject>
#include <QHash>
#include <QtGlobal>
#include <memory>

class Configuration;
class QQuickView;
class QScreen;
class PresentationClock;

class OverlayManager final : public QObject
{
    Q_OBJECT

public:
    explicit OverlayManager(Configuration *configuration, QObject *parent = nullptr);
    ~OverlayManager() override;

    bool show();
    void hide();
    bool isVisible() const;

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
    bool isDismissEvent(const QEvent *event) const;

    Configuration *m_configuration;
    AnimationState m_animationState;
    QHash<QScreen *, QQuickView *> m_views;
    QHash<QScreen *, PresentationClock *> m_presentationClocks;
    QScreen *m_animationDriverScreen = nullptr;
    qint64 m_animationEpochMs = 0;
    bool m_visible = false;
    bool m_sharedAnimationActive = false;
};

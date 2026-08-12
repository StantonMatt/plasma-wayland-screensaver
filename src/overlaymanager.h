// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once

#include <QObject>
#include <QHash>
#include <QtGlobal>
#include <memory>

class Configuration;
class QQuickView;
class QScreen;

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
    bool isDismissEvent(const QEvent *event) const;

    Configuration *m_configuration;
    QHash<QScreen *, QQuickView *> m_views;
    qint64 m_animationEpochMs = 0;
    bool m_visible = false;
};

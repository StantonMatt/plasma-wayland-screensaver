// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once

#include <QElapsedTimer>
#include <QObject>
#include <QPointF>
#include <QRectF>
#include <QRegion>
#include <QSizeF>
#include <QTimer>

class QScreen;

class AnimationState final : public QObject
{
    Q_OBJECT
    Q_PROPERTY(qreal ballX READ ballX NOTIFY frameChanged)
    Q_PROPERTY(qreal ballY READ ballY NOTIFY frameChanged)
    Q_PROPERTY(qreal ballSize READ ballSize NOTIFY frameChanged)
    Q_PROPERTY(qreal clockX READ clockX NOTIFY frameChanged)
    Q_PROPERTY(qreal clockY READ clockY NOTIFY frameChanged)

public:
    explicit AnimationState(QObject *parent = nullptr);

    void configure(const QList<QScreen *> &screens, bool animateBall, bool animateClock,
                   const QString &clockSpeed, int frameRate);
    void configureGeometries(const QList<QRect> &geometries, bool animateBall, bool animateClock,
                             const QString &clockSpeed, int frameRate);
    void stop();
    bool containsRect(const QRectF &rect) const;

    qreal ballX() const;
    qreal ballY() const;
    qreal ballSize() const;
    qreal clockX() const;
    qreal clockY() const;

    Q_INVOKABLE void setClockSize(qreal width, qreal height);

Q_SIGNALS:
    void frameChanged();

private Q_SLOTS:
    void advanceFrame();

private:
    struct Body {
        QPointF position;
        QPointF velocity;
        QSizeF size;
        bool initialized = false;
    };

    bool isValidPosition(const Body &body, const QPointF &position) const;
    void placeOnFirstScreen(Body &body, const QPointF &offset);
    void ensureValid(Body &body, const QPointF &offset);
    void advanceBody(Body &body, qreal seconds);
    void updateTimer();

    QRegion m_screenRegion;
    QList<QRect> m_screenGeometries;
    Body m_ball;
    Body m_clock;
    QTimer m_timer;
    QElapsedTimer m_elapsed;
    bool m_animateBall = false;
    bool m_animateClock = false;
};

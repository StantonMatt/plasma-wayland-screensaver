// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once

#include <QElapsedTimer>
#include <QObject>
#include <QPointF>
#include <QRectF>
#include <QRegion>
#include <QSizeF>
#include <QTimer>
#include <QVariantList>
#include <QVector>

class QScreen;

class AnimationState final : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QVariantList balls READ balls NOTIFY frameChanged)
    Q_PROPERTY(qreal ballSize READ ballSize NOTIFY frameChanged)
    Q_PROPERTY(qreal clockX READ clockX NOTIFY frameChanged)
    Q_PROPERTY(qreal clockY READ clockY NOTIFY frameChanged)

public:
    explicit AnimationState(QObject *parent = nullptr);

    void configure(const QList<QScreen *> &screens, bool animateBall, bool animateClock,
                   const QString &clockSpeed, int frameRate, int ballCount = 1,
                   int ballSpeed = 100, int ballScale = 100, int ballGravity = 0,
                   int ballElasticity = 100, bool ballCollisions = false,
                   bool externallyDriven = false);
    void configureGeometries(const QList<QRect> &geometries, bool animateBall, bool animateClock,
                             const QString &clockSpeed, int frameRate, int ballCount = 1,
                             int ballSpeed = 100, int ballScale = 100, int ballGravity = 0,
                             int ballElasticity = 100, bool ballCollisions = false,
                             bool externallyDriven = false);
    void stop();
    bool containsRect(const QRectF &rect) const;

    QVariantList balls() const;
    qreal ballSize() const;
    qreal clockX() const;
    qreal clockY() const;

    Q_INVOKABLE void setClockSize(qreal width, qreal height);
    void advance(qreal seconds);

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
    void advanceBody(Body &body, qreal seconds, bool applyGravity);
    void resolveBallCollisions();
    void rebuildBalls(int count, qreal size);
    void updateTimer();

    QRegion m_screenRegion;
    QList<QRect> m_screenGeometries;
    QVector<Body> m_balls;
    Body m_clock;
    QTimer m_timer;
    QElapsedTimer m_elapsed;
    bool m_animateBall = false;
    bool m_animateClock = false;
    qreal m_gravity = 0.0;
    qreal m_elasticity = 1.0;
    qreal m_motionSpeed = 1.0;
    bool m_ballCollisions = false;
    bool m_externallyDriven = false;
};

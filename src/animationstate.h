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
#include <array>
#include <optional>

class QScreen;

class AnimationState final : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QVariantList balls READ balls NOTIFY frameChanged)
    Q_PROPERTY(QRectF bounds READ bounds NOTIFY frameChanged)
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
    void advanceTo(qint64 presentationNanoseconds);
    Q_INVOKABLE QVariantList ballsAt(qint64 presentationNanoseconds) const;
    Q_INVOKABLE QPointF clockAt(qint64 presentationNanoseconds) const;
    QRectF bounds() const { return m_screenRegion.boundingRect(); }

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

    // 1 Hz is the minimum accepted refresh: at 60 Hz physics retain one
    // second of cross-window prediction skew and both interpolation endpoints.
    struct Frame {
        std::array<Body, 20> balls;
        Body clock;
        qreal time = 0;
        int count = 0;
    };
    static constexpr qreal physicsStep = 1.0 / 60.0;
    static constexpr size_t historySize = 63;
    void stepPhysics();
    void recordFrame();
    void resetHistory();
    Frame sample(qint64 presentationNanoseconds) const;
    static QVariantList ballList(const Body *balls, int count);
    bool isValidPosition(const Body &body, const QPointF &position) const;
    void placeOnFirstScreen(Body &body, const QPointF &offset);
    void ensureValid(Body &body, const QPointF &offset);
    void advanceBody(Body &body, qreal seconds, bool applyGravity);
    void resolveBallCollisions();
    void rebuildBalls(int count, qreal size);
    void updateTimer();

    std::array<Frame, historySize> m_history;
    size_t m_historyHead = 0;
    size_t m_historyCount = 0;
    qreal m_physicsTime = 0;
    qreal m_timelineTime = 0;
    std::optional<qint64> m_presentationNanoseconds;
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

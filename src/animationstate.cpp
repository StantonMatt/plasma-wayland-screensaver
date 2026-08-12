// SPDX-License-Identifier: GPL-3.0-or-later
#include "animationstate.h"

#include <QRectF>
#include <QScreen>
#include <QVariantMap>

#include <algorithm>
#include <cmath>

AnimationState::AnimationState(QObject *parent)
    : QObject(parent)
{
    m_timer.setTimerType(Qt::PreciseTimer);
    connect(&m_timer, &QTimer::timeout, this, &AnimationState::advanceFrame);
    m_clock.velocity = QPointF(31.0, 23.0);
    m_clock.size = QSizeF(360.0, 145.0);
}

void AnimationState::configure(const QList<QScreen *> &screens, bool animateBall,
                               bool animateClock, const QString &clockSpeed, int frameRate,
                               int ballCount, int ballSpeed, int ballScale, int ballGravity,
                               int ballElasticity, bool ballCollisions)
{
    QList<QRect> geometries;
    for (QScreen *screen : screens) {
        if (!screen) {
            continue;
        }
        geometries.append(screen->geometry());
    }
    configureGeometries(geometries, animateBall, animateClock, clockSpeed, frameRate,
                        ballCount, ballSpeed, ballScale, ballGravity, ballElasticity,
                        ballCollisions);
}

void AnimationState::configureGeometries(const QList<QRect> &geometries, bool animateBall,
                                         bool animateClock, const QString &clockSpeed, int frameRate,
                                         int ballCount, int ballSpeed, int ballScale,
                                         int ballGravity, int ballElasticity, bool ballCollisions)
{
    QRegion region;
    int shortestEdge = 1080;
    for (const QRect &geometry : geometries) {
        region += geometry;
        shortestEdge = std::min(shortestEdge, std::min(geometry.width(), geometry.height()));
    }
    m_screenRegion = region;
    m_screenGeometries = geometries;

    const qreal scale = std::clamp(ballScale, 25, 200) / 100.0;
    const qreal newBallSize = std::clamp(shortestEdge * 0.075 * scale, 24.0, 220.0);
    rebuildBalls(std::clamp(ballCount, 1, 20), newBallSize);
    const QPointF clockOffset = geometries.isEmpty() ? QPointF(180.0, 140.0)
        : QPointF(geometries.constFirst().width() * 0.28,
                  geometries.constFirst().height() * 0.22);
    for (qsizetype i = 0; i < m_balls.size(); ++i) {
        const qreal x = geometries.isEmpty() ? 60.0 + i * (newBallSize + 12.0)
            : std::fmod(geometries.constFirst().width() * 0.16 + i * newBallSize * 1.7,
                        std::max(newBallSize, geometries.constFirst().width() - newBallSize));
        const qreal y = geometries.isEmpty() ? 80.0
            : std::fmod(geometries.constFirst().height() * 0.17 + i * newBallSize * 1.13,
                        std::max(newBallSize, geometries.constFirst().height() - newBallSize));
        ensureValid(m_balls[i], QPointF(x, y));
    }
    ensureValid(m_clock, clockOffset);

    m_motionSpeed = std::clamp(ballSpeed, 10, 300) / 100.0;
    m_gravity = std::clamp(ballGravity, -100, 100) * 8.0;
    m_elasticity = std::clamp(ballElasticity, 50, 100) / 100.0;
    m_ballCollisions = ballCollisions;

    const qreal speedMultiplier = clockSpeed == QStringLiteral("slow") ? 0.55
        : (clockSpeed == QStringLiteral("fast") ? 1.8 : 1.0);
    const qreal xDirection = m_clock.velocity.x() < 0 ? -1.0 : 1.0;
    const qreal yDirection = m_clock.velocity.y() < 0 ? -1.0 : 1.0;
    m_clock.velocity = QPointF(31.0 * speedMultiplier * xDirection,
                               23.0 * speedMultiplier * yDirection);

    m_animateBall = animateBall;
    m_animateClock = animateClock;
    m_timer.setInterval(std::max(1, qRound(1000.0 / std::max(1, frameRate))));
    updateTimer();
    Q_EMIT frameChanged();
}

void AnimationState::stop()
{
    m_timer.stop();
    m_elapsed.invalidate();
    m_animateBall = false;
    m_animateClock = false;
}

bool AnimationState::containsRect(const QRectF &rect) const
{
    return !m_screenRegion.isEmpty()
        && QRegion(rect.toAlignedRect()).subtracted(m_screenRegion).isEmpty();
}

QVariantList AnimationState::balls() const
{
    QVariantList result;
    result.reserve(m_balls.size());
    for (qsizetype i = 0; i < m_balls.size(); ++i) {
        const Body &body = m_balls.at(i);
        result.append(QVariantMap{
            {QStringLiteral("x"), body.position.x()},
            {QStringLiteral("y"), body.position.y()},
            {QStringLiteral("size"), body.size.width()},
            {QStringLiteral("vx"), body.velocity.x()},
            {QStringLiteral("vy"), body.velocity.y()},
            {QStringLiteral("colorIndex"), i},
        });
    }
    return result;
}

qreal AnimationState::ballSize() const
{
    return m_balls.isEmpty() ? 80.0 : m_balls.constFirst().size.width();
}
qreal AnimationState::clockX() const { return m_clock.position.x(); }
qreal AnimationState::clockY() const { return m_clock.position.y(); }

void AnimationState::setClockSize(qreal width, qreal height)
{
    const QSizeF size(std::max(1.0, width), std::max(1.0, height));
    if (m_clock.size == size) {
        return;
    }
    m_clock.size = size;
    ensureValid(m_clock, QPointF(180.0, 140.0));
    Q_EMIT frameChanged();
}

bool AnimationState::isValidPosition(const Body &body, const QPointF &position) const
{
    if (m_screenRegion.isEmpty()) {
        return false;
    }
    return containsRect(QRectF(position, body.size));
}

void AnimationState::placeOnFirstScreen(Body &body, const QPointF &offset)
{
    if (m_screenGeometries.isEmpty()) {
        body.initialized = false;
        return;
    }
    const QRect geometry = m_screenGeometries.constFirst();
    const qreal maxX = geometry.right() + 1.0 - body.size.width();
    const qreal maxY = geometry.bottom() + 1.0 - body.size.height();
    body.position = QPointF(std::clamp(geometry.x() + offset.x(), qreal(geometry.x()), maxX),
                            std::clamp(geometry.y() + offset.y(), qreal(geometry.y()), maxY));
    body.initialized = true;
}

void AnimationState::ensureValid(Body &body, const QPointF &offset)
{
    if (!body.initialized || !isValidPosition(body, body.position)) {
        placeOnFirstScreen(body, offset);
    }
}

void AnimationState::advanceBody(Body &body, qreal seconds, bool applyGravity)
{
    if (!body.initialized || seconds <= 0) {
        return;
    }

    if (applyGravity) {
        body.velocity.ry() += m_gravity * seconds;
    }
    const qreal distance = std::max(std::abs(body.velocity.x()), std::abs(body.velocity.y())) * seconds;
    const int steps = std::max(1, qCeil(distance / 3.0));
    const qreal stepSeconds = seconds / steps;
    for (int step = 0; step < steps; ++step) {
        const QPointF delta = body.velocity * stepSeconds;
        const QPointF combined = body.position + delta;
        if (isValidPosition(body, combined)) {
            body.position = combined;
            continue;
        }

        const QPointF horizontal(body.position.x() + delta.x(), body.position.y());
        if (isValidPosition(body, horizontal)) {
            body.position = horizontal;
        } else {
            body.velocity.setX(-body.velocity.x() * (applyGravity ? m_elasticity : 1.0));
        }

        const QPointF vertical(body.position.x(), body.position.y() + delta.y());
        if (isValidPosition(body, vertical)) {
            body.position = vertical;
        } else {
            body.velocity.setY(-body.velocity.y() * (applyGravity ? m_elasticity : 1.0));
            if (applyGravity && std::abs(body.velocity.y()) < 28.0 && std::abs(m_gravity) > 1.0) {
                body.velocity.setY((m_gravity > 0 ? -1.0 : 1.0) * 72.0);
            }
        }
    }
}

void AnimationState::rebuildBalls(int count, qreal size)
{
    if (m_balls.size() == count && !m_balls.isEmpty()
        && qFuzzyCompare(m_balls.constFirst().size.width(), size)) {
        return;
    }
    m_balls.clear();
    m_balls.reserve(count);
    for (int i = 0; i < count; ++i) {
        Body body;
        const qreal sizeVariation = 0.78 + (i * 37 % 45) / 100.0;
        body.size = QSizeF(size * sizeVariation, size * sizeVariation);
        const qreal xDirection = i % 2 == 0 ? 1.0 : -1.0;
        const qreal yDirection = i % 3 == 0 ? -1.0 : 1.0;
        body.velocity = QPointF(xDirection * (118.0 + (i * 47) % 125),
                                yDirection * (86.0 + (i * 61) % 110));
        m_balls.append(body);
    }
}

void AnimationState::resolveBallCollisions()
{
    for (qsizetype i = 0; i < m_balls.size(); ++i) {
        for (qsizetype j = i + 1; j < m_balls.size(); ++j) {
            Body &a = m_balls[i];
            Body &b = m_balls[j];
            const QPointF centerA = a.position + QPointF(a.size.width() / 2, a.size.height() / 2);
            const QPointF centerB = b.position + QPointF(b.size.width() / 2, b.size.height() / 2);
            QPointF delta = centerB - centerA;
            qreal distance = std::hypot(delta.x(), delta.y());
            const qreal minimum = (a.size.width() + b.size.width()) / 2;
            if (distance >= minimum || distance < 0.001) {
                continue;
            }
            const QPointF normal = delta / distance;
            const qreal approaching = QPointF::dotProduct(b.velocity - a.velocity, normal);
            if (approaching < 0) {
                const qreal impulse = -(1.0 + m_elasticity) * approaching / 2.0;
                a.velocity -= normal * impulse;
                b.velocity += normal * impulse;
            }
            const qreal correction = (minimum - distance) / 2.0 + 0.1;
            const QPointF newA = a.position - normal * correction;
            const QPointF newB = b.position + normal * correction;
            if (isValidPosition(a, newA)) a.position = newA;
            if (isValidPosition(b, newB)) b.position = newB;
        }
    }
}

void AnimationState::advanceFrame()
{
    if (!m_elapsed.isValid()) {
        m_elapsed.start();
        return;
    }
    const qreal seconds = std::min(m_elapsed.nsecsElapsed() / 1'000'000'000.0, 0.05);
    m_elapsed.restart();
    if (m_animateBall) {
        for (Body &ball : m_balls) {
            advanceBody(ball, seconds * m_motionSpeed, true);
        }
        if (m_ballCollisions) {
            resolveBallCollisions();
        }
    }
    if (m_animateClock) {
        advanceBody(m_clock, seconds, false);
    }
    Q_EMIT frameChanged();
}

void AnimationState::updateTimer()
{
    if (!m_animateBall && !m_animateClock) {
        m_timer.stop();
        m_elapsed.invalidate();
        return;
    }
    m_elapsed.start();
    m_timer.start();
}

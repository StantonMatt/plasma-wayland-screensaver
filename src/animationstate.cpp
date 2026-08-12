// SPDX-License-Identifier: GPL-3.0-or-later
#include "animationstate.h"

#include <QRectF>
#include <QScreen>

#include <algorithm>
#include <cmath>

AnimationState::AnimationState(QObject *parent)
    : QObject(parent)
{
    m_timer.setTimerType(Qt::PreciseTimer);
    connect(&m_timer, &QTimer::timeout, this, &AnimationState::advanceFrame);
    m_ball.velocity = QPointF(176.0, 127.0);
    m_clock.velocity = QPointF(31.0, 23.0);
    m_clock.size = QSizeF(360.0, 145.0);
}

void AnimationState::configure(const QList<QScreen *> &screens, bool animateBall,
                               bool animateClock, const QString &clockSpeed, int frameRate)
{
    QList<QRect> geometries;
    for (QScreen *screen : screens) {
        if (!screen) {
            continue;
        }
        geometries.append(screen->geometry());
    }
    configureGeometries(geometries, animateBall, animateClock, clockSpeed, frameRate);
}

void AnimationState::configureGeometries(const QList<QRect> &geometries, bool animateBall,
                                         bool animateClock, const QString &clockSpeed, int frameRate)
{
    QRegion region;
    int shortestEdge = 1080;
    for (const QRect &geometry : geometries) {
        region += geometry;
        shortestEdge = std::min(shortestEdge, std::min(geometry.width(), geometry.height()));
    }
    m_screenRegion = region;
    m_screenGeometries = geometries;

    const qreal newBallSize = std::clamp(shortestEdge * 0.09, 56.0, 120.0);
    m_ball.size = QSizeF(newBallSize, newBallSize);
    const QPointF ballOffset = geometries.isEmpty() ? QPointF(60.0, 80.0)
        : QPointF(geometries.constFirst().width() * 0.72,
                  geometries.constFirst().height() * 0.35);
    const QPointF clockOffset = geometries.isEmpty() ? QPointF(180.0, 140.0)
        : QPointF(geometries.constFirst().width() * 0.28,
                  geometries.constFirst().height() * 0.22);
    ensureValid(m_ball, ballOffset);
    ensureValid(m_clock, clockOffset);

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

qreal AnimationState::ballX() const { return m_ball.position.x(); }
qreal AnimationState::ballY() const { return m_ball.position.y(); }
qreal AnimationState::ballSize() const { return m_ball.size.width(); }
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

void AnimationState::advanceBody(Body &body, qreal seconds)
{
    if (!body.initialized || seconds <= 0) {
        return;
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
            body.velocity.setX(-body.velocity.x());
        }

        const QPointF vertical(body.position.x(), body.position.y() + delta.y());
        if (isValidPosition(body, vertical)) {
            body.position = vertical;
        } else {
            body.velocity.setY(-body.velocity.y());
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
        advanceBody(m_ball, seconds);
    }
    if (m_animateClock) {
        advanceBody(m_clock, seconds);
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

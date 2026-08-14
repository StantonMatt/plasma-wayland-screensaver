// SPDX-License-Identifier: GPL-3.0-or-later
#include "snakerenderer.h"

#include <QSGGeometry>
#include <QSGGeometryNode>
#include <QSGVertexColorMaterial>

#include <algorithm>
#include <cmath>
#include <limits>
#include <utility>

namespace {
using Vertex = QSGGeometry::ColoredPoint2D;

qreal clamped(qreal value, qreal minimum, qreal maximum)
{
    return std::max(minimum, std::min(maximum, value));
}

qreal wrapped(qreal value, qreal extent)
{
    if (extent <= 0.0) {
        return 0.0;
    }
    value = std::fmod(value, extent);
    return value < 0.0 ? value + extent : value;
}

qreal axisDelta(qreal from, qreal to, qreal extent, bool deadlyWalls)
{
    qreal delta = to - from;
    if (!deadlyWalls && extent > 0.0) {
        if (delta > extent / 2.0) {
            delta -= extent;
        } else if (delta < -extent / 2.0) {
            delta += extent;
        }
    }
    return delta;
}

void appendVertex(QVector<Vertex> &vertices, const QPointF &point, const QColor &color)
{
    Vertex vertex;
    vertex.set(float(point.x()), float(point.y()),
               uchar(color.red()), uchar(color.green()), uchar(color.blue()), uchar(color.alpha()));
    vertices.append(vertex);
}

bool finitePoint(const QPointF &point)
{
    return std::isfinite(point.x()) && std::isfinite(point.y());
}

bool visible(const QRectF &bounds, const QSizeF &viewport)
{
    return bounds.intersects(QRectF(QPointF(0.0, 0.0), viewport));
}

void appendTriangle(QVector<Vertex> &vertices, const QPointF &a, const QPointF &b,
                    const QPointF &c, const QColor &color)
{
    // A single NaN sent to the graphics driver can invalidate the complete
    // triangle batch on some hardware. Drop only the malformed primitive so
    // the rest of the ecosystem keeps rendering while the simulation heals.
    if (!finitePoint(a) || !finitePoint(b) || !finitePoint(c)) {
        return;
    }
    appendVertex(vertices, a, color);
    appendVertex(vertices, b, color);
    appendVertex(vertices, c, color);
}

void appendDisc(QVector<Vertex> &vertices, const QPointF &center, qreal radius,
                const QColor &color, int sides, const QSizeF &viewport)
{
    if (!finitePoint(center) || !std::isfinite(radius) || radius <= 0.0
            || !visible(QRectF(center.x() - radius, center.y() - radius,
                              radius * 2.0, radius * 2.0), viewport)) {
        return;
    }
    constexpr qreal tau = 6.28318530717958647692;
    for (int side = 0; side < sides; ++side) {
        const qreal first = tau * side / sides;
        const qreal second = tau * (side + 1) / sides;
        appendTriangle(vertices, center,
                       center + QPointF(std::cos(first) * radius, std::sin(first) * radius),
                       center + QPointF(std::cos(second) * radius, std::sin(second) * radius),
                       color);
    }
}

void appendSegment(QVector<Vertex> &vertices, const QPointF &a, const QPointF &b,
                   qreal halfWidth, const QColor &color, const QSizeF &viewport)
{
    if (!finitePoint(a) || !finitePoint(b) || !std::isfinite(halfWidth)) {
        return;
    }
    const QPointF delta = b - a;
    const qreal length = std::hypot(delta.x(), delta.y());
    if (length < 0.001 || halfWidth <= 0.0) {
        return;
    }
    const QPointF normal(-delta.y() / length * halfWidth,
                         delta.x() / length * halfWidth);
    const QRectF bounds = QRectF(a, b).normalized().adjusted(-halfWidth, -halfWidth,
                                                             halfWidth, halfWidth);
    if (!visible(bounds, viewport)) {
        return;
    }
    appendTriangle(vertices, a + normal, a - normal, b + normal, color);
    appendTriangle(vertices, b + normal, a - normal, b - normal, color);
}

void appendRibbon(QVector<Vertex> &vertices, const QVector<QPointF> &points,
                  qreal halfWidth, const QColor &color, const QSizeF &viewport)
{
    if (points.size() < 2 || !std::isfinite(halfWidth) || halfWidth <= 0.0) {
        return;
    }

    QVector<QPointF> left(points.size());
    QVector<QPointF> right(points.size());
    QVector<bool> valid(points.size(), false);
    for (int index = 0; index < points.size(); ++index) {
        if (!finitePoint(points[index])) {
            continue;
        }
        const bool hasPrevious = index > 0 && finitePoint(points[index - 1]);
        const bool hasNext = index + 1 < points.size() && finitePoint(points[index + 1]);
        QPointF tangent;
        if (hasPrevious && hasNext) {
            // A centred tangent gives both adjoining quads the exact same
            // edge at the joint. This removes the wedge-shaped cracks that
            // independent segment quads require many discs to cover.
            tangent = points[index + 1] - points[index - 1];
        } else if (hasNext) {
            tangent = points[index + 1] - points[index];
        } else if (hasPrevious) {
            tangent = points[index] - points[index - 1];
        } else {
            continue;
        }
        const qreal length = std::hypot(tangent.x(), tangent.y());
        if (!std::isfinite(length) || length < 0.001) {
            continue;
        }
        const QPointF normal(-tangent.y() / length * halfWidth,
                             tangent.x() / length * halfWidth);
        left[index] = points[index] + normal;
        right[index] = points[index] - normal;
        valid[index] = true;
    }

    for (int index = 1; index < points.size(); ++index) {
        if (!valid[index - 1] || !valid[index]) {
            continue;
        }
        const QRectF bounds = QRectF(points[index - 1], points[index]).normalized()
            .adjusted(-halfWidth, -halfWidth, halfWidth, halfWidth);
        if (!visible(bounds, viewport)) {
            continue;
        }
        appendTriangle(vertices, left[index - 1], right[index - 1], left[index], color);
        appendTriangle(vertices, left[index], right[index - 1], right[index], color);
    }
}

QColor withAlpha(QColor color, int alpha)
{
    color.setAlpha(alpha);
    return color;
}

QVector<qreal> wrappingOffsets(qreal minimum, qreal maximum, qreal extent, qreal margin)
{
    QVector<qreal> offsets;
    if (extent <= 0.0) {
        offsets.append(0.0);
        return offsets;
    }
    const int first = int(std::ceil((-margin - maximum) / extent));
    const int last = int(std::floor((extent + margin - minimum) / extent));
    offsets.reserve(std::max(0, last - first + 1));
    for (int multiple = first; multiple <= last; ++multiple) {
        offsets.append(multiple * extent);
    }
    return offsets;
}
}

SnakeRenderer::SnakeRenderer(QQuickItem *parent)
    : QQuickItem(parent)
{
    setFlag(ItemHasContents, true);
}

void SnakeRenderer::syncFrame(const QJSValue &snakeValues, const QJSValue &foodValues,
                              const QJSValue &paletteValues, qreal simulationTime,
                              qreal interpolation, qreal worldWidth, qreal worldHeight,
                              qreal drawOffsetX, qreal drawOffsetY, bool deadlyWalls)
{
    setDrawOffset(drawOffsetX, drawOffsetY);
    if (m_source) {
        m_snakes = m_source->m_snakes;
        m_food = m_source->m_food;
        m_palette = m_source->m_palette;
        m_simulationTime = m_source->m_simulationTime;
        m_interpolation = m_source->m_interpolation;
        m_worldWidth = m_source->m_worldWidth;
        m_worldHeight = m_source->m_worldHeight;
        m_deadlyWalls = m_source->m_deadlyWalls;
        update();
        return;
    }

    const QVector<Snake> previousSnakes = m_snakes;
    const qreal elapsedSimulationTime = simulationTime - m_simulationTime;
    const bool onePhysicsStep = elapsedSimulationTime > 0.0
        && elapsedSimulationTime < 0.05;
    QVector<Snake> snakes;
    const int snakeCount = snakeValues.property(QStringLiteral("length")).toInt();
    snakes.reserve(snakeCount);
    for (int snakeIndex = 0; snakeIndex < snakeCount; ++snakeIndex) {
        const QJSValue value = snakeValues.property(snakeIndex);
        Snake snake;
        snake.alive = value.property(QStringLiteral("alive")).toBool();
        if (!snake.alive) {
            snakes.append(std::move(snake));
            continue;
        }
        snake.radius = value.property(QStringLiteral("radius")).toNumber();
        snake.angle = value.property(QStringLiteral("angle")).toNumber();
        snake.colorIndex = value.property(QStringLiteral("colorIndex")).toInt();
        const QJSValue segmentValues = value.property(QStringLiteral("segments"));
        const int segmentCount = segmentValues.property(QStringLiteral("length")).toInt();
        const Snake *previousSnake = onePhysicsStep && snakeIndex < previousSnakes.size()
                && previousSnakes[snakeIndex].alive
                && previousSnakes[snakeIndex].segments.size() == segmentCount
            ? &previousSnakes[snakeIndex] : nullptr;
        snake.segments.reserve(segmentCount);
        for (int segmentIndex = 0; segmentIndex < segmentCount; ++segmentIndex) {
            const QJSValue segmentValue = segmentValues.property(segmentIndex);
            Segment segment;
            segment.position = QPointF(segmentValue.property(QStringLiteral("x")).toNumber(),
                                       segmentValue.property(QStringLiteral("y")).toNumber());
            if (previousSnake) {
                segment.previous = previousSnake->segments[segmentIndex].position;
            } else {
                const QJSValue previousX = segmentValue.property(QStringLiteral("previousX"));
                const QJSValue previousY = segmentValue.property(QStringLiteral("previousY"));
                segment.previous = QPointF(previousX.isNumber() ? previousX.toNumber()
                                                                 : segment.position.x(),
                                           previousY.isNumber() ? previousY.toNumber()
                                                                 : segment.position.y());
            }
            snake.segments.append(segment);
        }
        snakes.append(std::move(snake));
    }

    QVector<Food> food;
    const int foodCount = foodValues.property(QStringLiteral("length")).toInt();
    food.reserve(foodCount);
    for (int foodIndex = 0; foodIndex < foodCount; ++foodIndex) {
        const QJSValue value = foodValues.property(foodIndex);
        Food particle;
        particle.position = QPointF(value.property(QStringLiteral("x")).toNumber(),
                                    value.property(QStringLiteral("y")).toNumber());
        particle.size = value.property(QStringLiteral("size")).toNumber();
        particle.phase = value.property(QStringLiteral("phase")).toNumber();
        particle.attraction = value.property(QStringLiteral("attraction")).toNumber();
        particle.attractionTarget = QPointF(
            value.property(QStringLiteral("attractionX")).toNumber(),
            value.property(QStringLiteral("attractionY")).toNumber());
        particle.colorIndex = value.property(QStringLiteral("colorIndex")).toInt();
        food.append(particle);
    }

    QVector<QColor> palette = m_palette;
    if (palette.isEmpty()) {
        const int colorCount = paletteValues.property(QStringLiteral("length")).toInt();
        palette.reserve(colorCount);
        for (int colorIndex = 0; colorIndex < colorCount; ++colorIndex) {
            const QColor color(paletteValues.property(colorIndex).toString());
            if (color.isValid()) {
                palette.append(color);
            }
        }
        if (palette.isEmpty()) {
            palette = {QColor(QStringLiteral("#4de6ff")), QColor(QStringLiteral("#b86cff")),
                       QColor(QStringLiteral("#ff4f8b"))};
        }
    }

    m_snakes = std::move(snakes);
    m_food = std::move(food);
    m_palette = std::move(palette);
    m_simulationTime = simulationTime;
    m_interpolation = clamped(interpolation, 0.0, 1.0);
    m_worldWidth = std::max(1.0, worldWidth);
    m_worldHeight = std::max(1.0, worldHeight);
    m_deadlyWalls = deadlyWalls;
    update();
    Q_EMIT frameSynchronized();
}

void SnakeRenderer::presentFrame(qreal simulationTime, qreal interpolation)
{
    if (!m_source) {
        m_simulationTime = simulationTime;
        m_interpolation = clamped(interpolation, 0.0, 1.0);
    }
    update();
    Q_EMIT frameSynchronized();
}

void SnakeRenderer::setDrawOffset(qreal drawOffsetX, qreal drawOffsetY)
{
    if (qFuzzyCompare(m_drawOffsetX, drawOffsetX)
            && qFuzzyCompare(m_drawOffsetY, drawOffsetY)) {
        return;
    }
    m_drawOffsetX = drawOffsetX;
    m_drawOffsetY = drawOffsetY;
    update();
}

void SnakeRenderer::setScaleToViewport(bool scaleToViewport)
{
    if (m_scaleToViewport == scaleToViewport) {
        return;
    }
    m_scaleToViewport = scaleToViewport;
    update();
}

void SnakeRenderer::follow(SnakeRenderer *source)
{
    if (m_source == source) {
        return;
    }
    disconnect(m_sourceConnection);
    m_source = source;
    if (!source) {
        m_sourceConnection = {};
        return;
    }
    const auto copyFrame = [this, source] {
        if (m_source != source) {
            return;
        }
        m_snakes = source->m_snakes;
        m_food = source->m_food;
        m_palette = source->m_palette;
        m_simulationTime = source->m_simulationTime;
        m_interpolation = source->m_interpolation;
        m_worldWidth = source->m_worldWidth;
        m_worldHeight = source->m_worldHeight;
        m_deadlyWalls = source->m_deadlyWalls;
        update();
    };
    m_sourceConnection = connect(source, &SnakeRenderer::frameSynchronized,
                                 this, copyFrame);
    copyFrame();
}

QSGNode *SnakeRenderer::updatePaintNode(QSGNode *oldNode,
                                        UpdatePaintNodeData *updatePaintNodeData)
{
    Q_UNUSED(updatePaintNodeData)
    auto *node = static_cast<QSGGeometryNode *>(oldNode);
    if (!node) {
        node = new QSGGeometryNode;
        auto *geometry = new QSGGeometry(QSGGeometry::defaultAttributes_ColoredPoint2D(), 0);
        geometry->setDrawingMode(QSGGeometry::DrawTriangles);
        geometry->setVertexDataPattern(QSGGeometry::DynamicPattern);
        node->setGeometry(geometry);
        node->setFlag(QSGNode::OwnsGeometry);
        auto *material = new QSGVertexColorMaterial;
        material->setFlag(QSGMaterial::Blending, true);
        node->setMaterial(material);
        node->setFlag(QSGNode::OwnsMaterial);
        m_geometryCapacity = 0;
    }

    QVector<Vertex> vertices;
    qsizetype liveSegmentCount = 0;
    for (const Snake &snake : std::as_const(m_snakes)) {
        if (snake.alive) {
            liveSegmentCount += snake.segments.size();
        }
    }
    // Body ribbons, join discs, markings, eyes and crowns average fewer than
    // 42 vertices per segment. Reserving from actual segment count prevents
    // repeated CPU-side reallocations as a champion grows.
    const qsizetype estimatedVertices = m_food.size() * 72
        + liveSegmentCount * 42 + m_snakes.size() * 180;
    vertices.reserve(std::min<qsizetype>(estimatedVertices,
                                         std::numeric_limits<int>::max()));
    const QSizeF viewport(width(), height());
    const QPointF drawOffset(m_drawOffsetX, m_drawOffsetY);
    const qreal scaleX = m_scaleToViewport
        ? viewport.width() / std::max(1.0, m_worldWidth) : 1.0;
    const qreal scaleY = m_scaleToViewport
        ? viewport.height() / std::max(1.0, m_worldHeight) : 1.0;
    const qreal sizeScale = std::sqrt(scaleX * scaleY);
    const auto mapPoint = [drawOffset, scaleX, scaleY](const QPointF &point) {
        return QPointF((point.x() + drawOffset.x()) * scaleX,
                       (point.y() + drawOffset.y()) * scaleY);
    };

    const bool denseFood = m_food.size() > 320;
    for (const Food &particle : std::as_const(m_food)) {
        const qreal pulse = 0.82 + std::sin(m_simulationTime * 3.0 + particle.phase) * 0.18;
        const qreal worldSize = particle.size * pulse;
        const qreal size = worldSize * sizeScale;
        const QColor color = m_palette.at(particle.colorIndex % m_palette.size());
        QVector<qreal> xOffsets{0.0};
        QVector<qreal> yOffsets{0.0};
        if (!m_deadlyWalls) {
            if (particle.position.x() < worldSize * 3.2) xOffsets.append(m_worldWidth);
            if (particle.position.x() > m_worldWidth - worldSize * 3.2) xOffsets.append(-m_worldWidth);
            if (particle.position.y() < worldSize * 3.2) yOffsets.append(m_worldHeight);
            if (particle.position.y() > m_worldHeight - worldSize * 3.2) yOffsets.append(-m_worldHeight);
        }
        for (qreal xOffset : std::as_const(xOffsets)) {
            for (qreal yOffset : std::as_const(yOffsets)) {
                const QPointF center = mapPoint(particle.position + QPointF(xOffset, yOffset));
                if (particle.attraction > 0.0) {
                    const QPointF worldPull(
                        axisDelta(particle.position.x(), particle.attractionTarget.x(),
                                  m_worldWidth, m_deadlyWalls),
                        axisDelta(particle.position.y(), particle.attractionTarget.y(),
                                  m_worldHeight, m_deadlyWalls));
                    const QPointF pull(worldPull.x() * scaleX, worldPull.y() * scaleY);
                    const qreal pullLength = std::hypot(pull.x(), pull.y());
                    if (pullLength > 0.001) {
                        const qreal trailLength = size * (2.0 + particle.attraction * 5.0);
                        appendSegment(vertices, center,
                                      center - pull / pullLength * trailLength,
                                      std::max(0.5, size * 0.38),
                                      withAlpha(color, 150), viewport);
                    }
                }
                // Once several death trails overlap, the glow layer dominates
                // vertex count without adding useful visual information. Keep
                // the bright edible cores and magnetic trails intact while
                // dropping only this overdraw-heavy decoration.
                if (!denseFood) {
                    appendDisc(vertices, center, size * 3.2,
                               withAlpha(color, 30), 8, viewport);
                }
                appendDisc(vertices, center, size, withAlpha(color, 230),
                           denseFood ? 6 : 8, viewport);
                appendDisc(vertices, center - QPointF(size * 0.24, size * 0.24),
                           std::max(0.7, size * 0.3), QColor(255, 255, 255, 215),
                           denseFood ? 4 : 5, viewport);
            }
        }
    }

    int leaderLength = 0;
    for (const Snake &snake : std::as_const(m_snakes)) {
        if (snake.alive) {
            leaderLength = std::max(leaderLength, int(snake.segments.size()));
        }
    }
    for (const Snake &snake : std::as_const(m_snakes)) {
        if (!snake.alive || snake.segments.size() < 2) {
            continue;
        }
        QVector<QPointF> points;
        points.reserve(snake.segments.size());
        for (const Segment &segment : snake.segments) {
            qreal x = segment.previous.x()
                + axisDelta(segment.previous.x(), segment.position.x(), m_worldWidth,
                            m_deadlyWalls) * m_interpolation;
            qreal y = segment.previous.y()
                + axisDelta(segment.previous.y(), segment.position.y(), m_worldHeight,
                            m_deadlyWalls) * m_interpolation;
            if (!m_deadlyWalls) {
                x = wrapped(x, m_worldWidth);
                y = wrapped(y, m_worldHeight);
            }
            if (!points.isEmpty() && !m_deadlyWalls) {
                x = points.constLast().x()
                    + axisDelta(wrapped(points.constLast().x(), m_worldWidth), x,
                                m_worldWidth, false);
                y = points.constLast().y()
                    + axisDelta(wrapped(points.constLast().y(), m_worldHeight), y,
                                m_worldHeight, false);
            }
            points.append(QPointF(x, y));
        }

        qreal minimumX = points.constFirst().x();
        qreal maximumX = minimumX;
        qreal minimumY = points.constFirst().y();
        qreal maximumY = minimumY;
        for (const QPointF &point : std::as_const(points)) {
            minimumX = std::min(minimumX, point.x());
            maximumX = std::max(maximumX, point.x());
            minimumY = std::min(minimumY, point.y());
            maximumY = std::max(maximumY, point.y());
        }
        QVector<qreal> xOffsets{0.0};
        QVector<qreal> yOffsets{0.0};
        if (!m_deadlyWalls) {
            const qreal margin = snake.radius * 3.0;
            xOffsets = wrappingOffsets(minimumX, maximumX, m_worldWidth, margin);
            yOffsets = wrappingOffsets(minimumY, maximumY, m_worldHeight, margin);
        }

        const QColor color = m_palette.at(snake.colorIndex % m_palette.size());
        const QColor outline(5, 7, 16, 175);
        const qreal radius = snake.radius * sizeScale;
        const qreal bodyMargin = radius * 1.4;
        for (qreal xOffset : std::as_const(xOffsets)) {
            for (qreal yOffset : std::as_const(yOffsets)) {
                const QPointF wrapOffset(xOffset, yOffset);
                const QRectF copyBounds(
                    (minimumX + xOffset + drawOffset.x()) * scaleX - bodyMargin,
                    (minimumY + yOffset + drawOffset.y()) * scaleY - bodyMargin,
                    (maximumX - minimumX) * scaleX + bodyMargin * 2.0,
                    (maximumY - minimumY) * scaleY + bodyMargin * 2.0);
                // In seamless mode most snakes are outside any one monitor.
                // Reject a whole wrapped copy before walking its body and
                // attempting to append/cull every individual primitive.
                if (!visible(copyBounds, viewport)) {
                    continue;
                }
                QVector<QPointF> renderedPoints;
                renderedPoints.reserve(points.size());
                for (const QPointF &point : std::as_const(points)) {
                    renderedPoints.append(mapPoint(point + wrapOffset));
                }
                appendRibbon(vertices, renderedPoints, radius * 1.275,
                             outline, viewport);
                appendDisc(vertices, renderedPoints.constLast(), radius * 1.275,
                           outline, 12, viewport);
                appendRibbon(vertices, renderedPoints, radius * 0.96,
                             withAlpha(color, 245), viewport);
                appendDisc(vertices, renderedPoints.constLast(), radius * 0.96,
                           withAlpha(color, 245), 12, viewport);
                for (int index = 5; index < points.size(); index += 6) {
                    appendDisc(vertices, renderedPoints[index], radius * 0.34,
                               QColor(255, 255, 255, 46), 6, viewport);
                }

                const QPointF head = renderedPoints.constFirst();
                appendDisc(vertices, head, radius * 1.08,
                           withAlpha(color, 255), 12, viewport);
                QPointF forward(std::cos(snake.angle) * scaleX,
                                std::sin(snake.angle) * scaleY);
                const qreal forwardLength = std::hypot(forward.x(), forward.y());
                if (forwardLength > 0.001) {
                    forward /= forwardLength;
                }
                const QPointF side(-forward.y(), forward.x());
                const qreal eyeRadius = std::max(1.7, radius * 0.31);
                for (int direction : {-1, 1}) {
                    const QPointF eye = head + forward * (radius * 0.48)
                        + side * (radius * 0.46 * direction);
                    appendDisc(vertices, eye, eyeRadius, Qt::white, 8, viewport);
                    appendDisc(vertices, eye + forward * (eyeRadius * 0.34),
                               eyeRadius * 0.48, QColor(QStringLiteral("#11131a")),
                               7, viewport);
                }
                if (points.size() == leaderLength) {
                    const QPointF crownCenter = head - forward * (radius * 0.32);
                    const QVector<QPointF> crown{
                        crownCenter - side * (radius * 0.82)
                            - forward * (radius * 0.42),
                        crownCenter - side * (radius * 0.82)
                            + forward * (radius * 0.58),
                        crownCenter - side * (radius * 0.34)
                            + forward * (radius * 0.18),
                        crownCenter + forward * (radius * 0.98),
                        crownCenter + side * (radius * 0.34)
                            + forward * (radius * 0.18),
                        crownCenter + side * (radius * 0.82)
                            + forward * (radius * 0.58),
                        crownCenter + side * (radius * 0.82)
                            - forward * (radius * 0.42)};
                    const QColor gold(QStringLiteral("#ffd84a"));
                    for (int index = 0; index < crown.size(); ++index) {
                        appendTriangle(vertices, crownCenter, crown[index],
                                       crown[(index + 1) % crown.size()], gold);
                    }
                    for (int index = 0; index < crown.size(); ++index) {
                        appendSegment(vertices, crown[index],
                                      crown[(index + 1) % crown.size()],
                                      std::max(0.65, radius * 0.09),
                                      QColor(QStringLiteral("#6d4300")), viewport);
                    }
                    appendDisc(vertices, crownCenter + forward * (radius * 0.04),
                               std::max(0.8, radius * 0.12),
                               QColor(QStringLiteral("#fff2a0")), 6, viewport);
                }
            }
        }
    }

    QSGGeometry *geometry = node->geometry();
    const int vertexCount = vertices.size();
#if QT_VERSION >= QT_VERSION_CHECK(6, 10, 0)
    if (vertexCount > m_geometryCapacity) {
        // Visible counts vary whenever food expires or a snake crosses a
        // monitor edge. Keep spare GPU-buffer capacity so those routine
        // changes do not destroy and recreate the scene-graph allocation,
        // which can present as a one-frame blank/flicker on some drivers.
        const int grownCapacity = m_geometryCapacity > 0
            ? m_geometryCapacity + std::max(1024, m_geometryCapacity / 2)
            : 4096;
        m_geometryCapacity = std::max(vertexCount, grownCapacity);
        geometry->allocate(m_geometryCapacity);
    }
    geometry->setVertexCount(vertexCount);
#else
    // setVertexCount() was introduced in Qt 6.10. Keep the project buildable
    // with its Qt 6.8 baseline, albeit without retained-capacity optimization.
    geometry->allocate(vertexCount);
    m_geometryCapacity = vertexCount;
#endif
    if (!vertices.isEmpty()) {
        std::copy(vertices.cbegin(), vertices.cend(), geometry->vertexDataAsColoredPoint2D());
    }
    geometry->markVertexDataDirty();
    node->markDirty(QSGNode::DirtyGeometry);
    return node;
}

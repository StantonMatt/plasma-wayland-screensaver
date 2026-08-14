// SPDX-License-Identifier: GPL-3.0-or-later
#include "snakerenderer.h"

#include <QSGGeometry>
#include <QSGGeometryNode>
#include <QSGVertexColorMaterial>

#include <algorithm>
#include <cmath>
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

bool visible(const QRectF &bounds, const QSizeF &viewport)
{
    return bounds.intersects(QRectF(QPointF(0.0, 0.0), viewport));
}

void appendTriangle(QVector<Vertex> &vertices, const QPointF &a, const QPointF &b,
                    const QPointF &c, const QColor &color)
{
    appendVertex(vertices, a, color);
    appendVertex(vertices, b, color);
    appendVertex(vertices, c, color);
}

void appendDisc(QVector<Vertex> &vertices, const QPointF &center, qreal radius,
                const QColor &color, int sides, const QSizeF &viewport)
{
    if (radius <= 0.0
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
        snake.segments.reserve(segmentCount);
        for (int segmentIndex = 0; segmentIndex < segmentCount; ++segmentIndex) {
            const QJSValue segmentValue = segmentValues.property(segmentIndex);
            Segment segment;
            segment.position = QPointF(segmentValue.property(QStringLiteral("x")).toNumber(),
                                       segmentValue.property(QStringLiteral("y")).toNumber());
            const QJSValue previousX = segmentValue.property(QStringLiteral("previousX"));
            const QJSValue previousY = segmentValue.property(QStringLiteral("previousY"));
            segment.previous = QPointF(previousX.isNumber() ? previousX.toNumber()
                                                             : segment.position.x(),
                                       previousY.isNumber() ? previousY.toNumber()
                                                             : segment.position.y());
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

    QVector<QColor> palette;
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
    }

    QVector<Vertex> vertices;
    vertices.reserve(m_food.size() * 48 + m_snakes.size() * 600);
    const QSizeF viewport(width(), height());
    const QPointF drawOffset(m_drawOffsetX, m_drawOffsetY);

    for (const Food &particle : std::as_const(m_food)) {
        const qreal pulse = 0.82 + std::sin(m_simulationTime * 3.0 + particle.phase) * 0.18;
        const qreal size = particle.size * pulse;
        const QColor color = m_palette.at(particle.colorIndex % m_palette.size());
        QVector<qreal> xOffsets{0.0};
        QVector<qreal> yOffsets{0.0};
        if (!m_deadlyWalls) {
            if (particle.position.x() < size * 3.2) xOffsets.append(m_worldWidth);
            if (particle.position.x() > m_worldWidth - size * 3.2) xOffsets.append(-m_worldWidth);
            if (particle.position.y() < size * 3.2) yOffsets.append(m_worldHeight);
            if (particle.position.y() > m_worldHeight - size * 3.2) yOffsets.append(-m_worldHeight);
        }
        for (qreal xOffset : std::as_const(xOffsets)) {
            for (qreal yOffset : std::as_const(yOffsets)) {
                const QPointF center = particle.position + drawOffset + QPointF(xOffset, yOffset);
                if (particle.attraction > 0.0) {
                    const QPointF pull(
                        axisDelta(particle.position.x(), particle.attractionTarget.x(),
                                  m_worldWidth, m_deadlyWalls),
                        axisDelta(particle.position.y(), particle.attractionTarget.y(),
                                  m_worldHeight, m_deadlyWalls));
                    const qreal pullLength = std::hypot(pull.x(), pull.y());
                    if (pullLength > 0.001) {
                        const qreal trailLength = size * (2.0 + particle.attraction * 5.0);
                        appendSegment(vertices, center,
                                      center - pull / pullLength * trailLength,
                                      std::max(0.5, size * 0.38),
                                      withAlpha(color, 150), viewport);
                    }
                }
                appendDisc(vertices, center, size * 3.2, withAlpha(color, 30), 8, viewport);
                appendDisc(vertices, center, size, withAlpha(color, 230), 8, viewport);
                appendDisc(vertices, center - QPointF(size * 0.24, size * 0.24),
                           std::max(0.7, size * 0.3), QColor(255, 255, 255, 215), 5, viewport);
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
        for (qreal xOffset : std::as_const(xOffsets)) {
            for (qreal yOffset : std::as_const(yOffsets)) {
                const QPointF offset = drawOffset + QPointF(xOffset, yOffset);
                for (int index = 1; index < points.size(); ++index) {
                    appendSegment(vertices, points[index - 1] + offset, points[index] + offset,
                                  snake.radius * 1.275, outline, viewport);
                }
                for (int index = 1; index < points.size(); ++index) {
                    appendDisc(vertices, points[index] + offset, snake.radius * 1.275,
                               outline, 6, viewport);
                }
                appendDisc(vertices, points.constLast() + offset, snake.radius * 1.275,
                           outline, 12, viewport);
                for (int index = 1; index < points.size(); ++index) {
                    appendSegment(vertices, points[index - 1] + offset, points[index] + offset,
                                  snake.radius * 0.96, withAlpha(color, 245), viewport);
                }
                for (int index = 1; index < points.size(); ++index) {
                    appendDisc(vertices, points[index] + offset, snake.radius * 0.96,
                               withAlpha(color, 245), 6, viewport);
                }
                appendDisc(vertices, points.constLast() + offset, snake.radius * 0.96,
                           withAlpha(color, 245), 12, viewport);
                for (int index = 5; index < points.size(); index += 6) {
                    appendDisc(vertices, points[index] + offset, snake.radius * 0.34,
                               QColor(255, 255, 255, 46), 6, viewport);
                }

                const QPointF head = points.constFirst() + offset;
                appendDisc(vertices, head, snake.radius * 1.08,
                           withAlpha(color, 255), 12, viewport);
                const QPointF forward(std::cos(snake.angle), std::sin(snake.angle));
                const QPointF side(-forward.y(), forward.x());
                const qreal eyeRadius = std::max(1.7, snake.radius * 0.31);
                for (int direction : {-1, 1}) {
                    const QPointF eye = head + forward * (snake.radius * 0.48)
                        + side * (snake.radius * 0.46 * direction);
                    appendDisc(vertices, eye, eyeRadius, Qt::white, 8, viewport);
                    appendDisc(vertices, eye + forward * (eyeRadius * 0.34),
                               eyeRadius * 0.48, QColor(QStringLiteral("#11131a")),
                               7, viewport);
                }
                if (points.size() == leaderLength) {
                    const QPointF crownCenter = head - forward * (snake.radius * 0.32);
                    const QVector<QPointF> crown{
                        crownCenter - side * (snake.radius * 0.82)
                            - forward * (snake.radius * 0.42),
                        crownCenter - side * (snake.radius * 0.82)
                            + forward * (snake.radius * 0.58),
                        crownCenter - side * (snake.radius * 0.34)
                            + forward * (snake.radius * 0.18),
                        crownCenter + forward * (snake.radius * 0.98),
                        crownCenter + side * (snake.radius * 0.34)
                            + forward * (snake.radius * 0.18),
                        crownCenter + side * (snake.radius * 0.82)
                            + forward * (snake.radius * 0.58),
                        crownCenter + side * (snake.radius * 0.82)
                            - forward * (snake.radius * 0.42)};
                    const QColor gold(QStringLiteral("#ffd84a"));
                    for (int index = 0; index < crown.size(); ++index) {
                        appendTriangle(vertices, crownCenter, crown[index],
                                       crown[(index + 1) % crown.size()], gold);
                    }
                    for (int index = 0; index < crown.size(); ++index) {
                        appendSegment(vertices, crown[index],
                                      crown[(index + 1) % crown.size()],
                                      std::max(0.65, snake.radius * 0.09),
                                      QColor(QStringLiteral("#6d4300")), viewport);
                    }
                    appendDisc(vertices, crownCenter + forward * (snake.radius * 0.04),
                               std::max(0.8, snake.radius * 0.12),
                               QColor(QStringLiteral("#fff2a0")), 6, viewport);
                }
            }
        }
    }

    QSGGeometry *geometry = node->geometry();
    geometry->allocate(vertices.size());
    if (!vertices.isEmpty()) {
        std::copy(vertices.cbegin(), vertices.cend(), geometry->vertexDataAsColoredPoint2D());
    }
    node->markDirty(QSGNode::DirtyGeometry);
    return node;
}

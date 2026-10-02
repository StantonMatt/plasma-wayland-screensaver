// SPDX-License-Identifier: GPL-3.0-or-later
#include "snakerenderer.h"

#include <QSGGeometry>
#include <QSGGeometryNode>
#include <QSGVertexColorMaterial>

#include <algorithm>
#include <cmath>
#include <limits>
#include <utility>
#include <array>
#include <span>
#include <vector>

namespace {
using Vertex = QSGGeometry::ColoredPoint2D;

// QColor's channel accessors perform format conversion. Do it once per
// primitive, rather than four times for every vertex in its triangles.
struct VertexColor {
    uchar red, green, blue, alpha;
    VertexColor(const QColor &color) {
        const QRgb rgba = color.rgba();
        red = qRed(rgba); green = qGreen(rgba);
        blue = qBlue(rgba); alpha = qAlpha(rgba);
    }
};

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

void appendVertex(std::vector<Vertex> &vertices, const QPointF &point, VertexColor color)
{
    Vertex vertex;
    vertex.set(float(point.x()), float(point.y()),
               color.red, color.green, color.blue, color.alpha);
    vertices.push_back(vertex);
}

bool finitePoint(const QPointF &point)
{
    return std::isfinite(point.x()) && std::isfinite(point.y());
}

bool visible(const QRectF &bounds, const QSizeF &viewport)
{
    return bounds.intersects(QRectF(QPointF(0.0, 0.0), viewport));
}

std::vector<QPointF> makeUnitCircle(int sides)
{
    constexpr qreal tau = 6.28318530717958647692;
    std::vector<QPointF> points;
    points.reserve(sides + 1);
    for (int side = 0; side <= sides; ++side) {
        const qreal angle = tau * side / sides;
        points.push_back(QPointF(std::cos(angle), std::sin(angle)));
    }
    return points;
}

const std::vector<QPointF> *cachedUnitCircle(int sides)
{
    // Every disc in the renderer uses one of these detail levels. Cache their
    // unit vertices once instead of evaluating thousands of identical sine
    // and cosine pairs on every presentation frame.
    static const std::vector<QPointF> four = makeUnitCircle(4);
    static const std::vector<QPointF> five = makeUnitCircle(5);
    static const std::vector<QPointF> six = makeUnitCircle(6);
    static const std::vector<QPointF> seven = makeUnitCircle(7);
    static const std::vector<QPointF> eight = makeUnitCircle(8);
    static const std::vector<QPointF> twelve = makeUnitCircle(12);
    switch (sides) {
    case 4: return &four;
    case 5: return &five;
    case 6: return &six;
    case 7: return &seven;
    case 8: return &eight;
    case 12: return &twelve;
    default: return nullptr;
    }
}

void appendTriangle(std::vector<Vertex> &vertices, const QPointF &a, const QPointF &b,
                    const QPointF &c, VertexColor color)
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

void appendDisc(std::vector<Vertex> &vertices, const QPointF &center, qreal radius,
                VertexColor color, int sides, const QSizeF &viewport)
{
    if (!finitePoint(center) || !std::isfinite(radius) || radius <= 0.0
            || !visible(QRectF(center.x() - radius, center.y() - radius,
                              radius * 2.0, radius * 2.0), viewport)) {
        return;
    }
    const std::vector<QPointF> *unitPoints = cachedUnitCircle(sides);
    for (int side = 0; side < sides; ++side) {
        if (unitPoints) {
            appendVertex(vertices, center, color);
            appendVertex(vertices, center + unitPoints->at(side) * radius, color);
            appendVertex(vertices, center + unitPoints->at(side + 1) * radius, color);
        } else {
            constexpr qreal tau = 6.28318530717958647692;
            const qreal first = tau * side / sides;
            const qreal second = tau * (side + 1) / sides;
            appendVertex(vertices, center, color);
            appendVertex(vertices, center + QPointF(std::cos(first) * radius,
                                                     std::sin(first) * radius), color);
            appendVertex(vertices, center + QPointF(std::cos(second) * radius,
                                                     std::sin(second) * radius), color);
        }
    }
}

void appendSegment(std::vector<Vertex> &vertices, const QPointF &a, const QPointF &b,
                   qreal halfWidth, VertexColor color, const QSizeF &viewport)
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
    appendVertex(vertices, a + normal, color);
    appendVertex(vertices, a - normal, color);
    appendVertex(vertices, b + normal, color);
    appendVertex(vertices, b + normal, color);
    appendVertex(vertices, a - normal, color);
    appendVertex(vertices, b - normal, color);
}

void prepareRibbon(const std::vector<QPointF> &points, std::vector<QPointF> &normals,
                   std::vector<quint8> &valid)
{
    if (points.size() < 2) {
        return;
    }

    normals.resize(points.size());
    valid.resize(points.size());
    std::fill(valid.begin(), valid.end(), 0);
    for (size_t index = 0; index < points.size(); ++index) {
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
        normals[index] = QPointF(-tangent.y() / length, tangent.x() / length);
        valid[index] = true;
    }
}

void appendRibbon(std::vector<Vertex> &vertices, const std::vector<QPointF> &points,
                  const std::vector<QPointF> &normals, const std::vector<quint8> &valid,
                  qreal halfWidth, VertexColor color, const QSizeF &viewport)
{
    if (points.size() < 2 || !std::isfinite(halfWidth) || halfWidth <= 0.0) return;
    for (size_t index = 1; index < points.size(); ++index) {
        if (!valid[index - 1] || !valid[index]) {
            continue;
        }
        const QRectF bounds = QRectF(points[index - 1], points[index]).normalized()
            .adjusted(-halfWidth, -halfWidth, halfWidth, halfWidth);
        if (!visible(bounds, viewport)) {
            continue;
        }
        const QPointF aNormal = normals[index - 1] * halfWidth;
        const QPointF bNormal = normals[index] * halfWidth;
        const QPointF aLeft = points[index - 1] + aNormal;
        const QPointF aRight = points[index - 1] - aNormal;
        const QPointF bLeft = points[index] + bNormal;
        const QPointF bRight = points[index] - bNormal;
        appendVertex(vertices, aLeft, color);
        appendVertex(vertices, aRight, color);
        appendVertex(vertices, bLeft, color);
        appendVertex(vertices, bLeft, color);
        appendVertex(vertices, aRight, color);
        appendVertex(vertices, bRight, color);
    }
}

QColor withAlpha(QColor color, int alpha)
{
    color.setAlpha(alpha);
    return color;
}

struct Offsets {
    int first = 0, last = 0;
    qreal extent = 0;
};
Offsets wrappingOffsets(qreal minimum, qreal maximum, qreal extent, qreal margin)
{
    if (extent <= 0) return {};
    return {int(std::ceil((-margin - maximum) / extent)),
            int(std::floor((extent + margin - minimum) / extent)), extent};
}

struct GeometryNode : QSGGeometryNode {
    std::vector<Vertex> vertices;
    std::vector<QPointF> points, renderedPoints, normals;
    std::vector<quint8> valid;
};
struct Segment {
    QPointF position, previous;
    Segment(const snakes_core_segment &s) : position(s.x, s.y), previous(s.previous_x, s.previous_y) {}
};
struct Snake {
    std::span<const snakes_core_segment> segments;
    qreal radius, angle, desiredAngle;
    int colorIndex;
    bool alive;
    Snake(const snakes_core_snake &s, const SnakeFrame &frame)
        : segments(std::span(frame.segments).subspan(s.segment_offset, s.segment_count)),
          radius(s.radius), angle(s.angle), desiredAngle(s.desired_angle),
          colorIndex(s.color_index), alive(s.alive) {}
};
struct Food {
    QPointF position, attractionTarget;
    qreal size, phase, attraction;
    int colorIndex;
    Food(const snakes_core_food &f) : position(f.x, f.y), attractionTarget(f.attraction_x, f.attraction_y),
        size(f.size), phase(f.phase), attraction(f.attraction), colorIndex(f.color_index) {}
};

}

SnakeRenderer::SnakeRenderer(QQuickItem *parent)
    : QQuickItem(parent)
{
    setFlag(ItemHasContents, true);
}

void SnakeRenderer::setSimulation(SnakeSimulation *simulation)
{
    if (m_simulation == simulation) return;
    disconnect(m_presentedConnection);
    disconnect(m_destroyedConnection);
    m_simulation = simulation;
    m_frame = nullptr;
    m_retainedFrame.reset();
    m_presentationNanoseconds.reset();
    if (simulation) {
        const auto sync = [this, simulation] {
            syncFrame(simulation->frame(), simulation->palette(), simulation->interpolation(),
                      simulation->config().deadly_walls);
            m_retainedFrame = simulation->retainFrame();
        };
        m_presentedConnection = connect(simulation, &SnakeSimulation::presented, this, sync);
        m_destroyedConnection = connect(simulation, &QObject::destroyed, this, [this] {
            m_frame = nullptr;
            m_retainedFrame.reset();
            m_presentationNanoseconds.reset();
            update();
            Q_EMIT simulationChanged();
        });
        sync();
    }
    update();
    Q_EMIT simulationChanged();
}

void SnakeRenderer::syncFrame(const SnakeFrame &frame, const QVector<QColor> &palette,
                              qreal interpolation, bool deadlyWalls)
{
    m_retainedFrame.reset();
    m_presentationNanoseconds.reset();
    loadFrame(frame, palette, interpolation, deadlyWalls);
    update();
}

void SnakeRenderer::loadFrame(const SnakeFrame &frame, const QVector<QColor> &palette,
                              qreal interpolation, bool deadlyWalls)
{
    m_frame = &frame;
    m_palette = palette;
    m_simulationTime = frame.info.simulation_time;
    m_interpolation = clamped(interpolation, 0.0, 1.0);
    m_worldWidth = std::max(1.0, frame.info.world_width);
    m_worldHeight = std::max(1.0, frame.info.world_height);
    m_worldToViewX = m_simulation ? m_simulation->viewSize().width() / m_worldWidth : 1.0;
    m_worldToViewY = m_simulation ? m_simulation->viewSize().height() / m_worldHeight : 1.0;
    m_deadlyWalls = deadlyWalls;
}

void SnakeRenderer::presentAt(qint64 presentationNanoseconds)
{
    if (!m_simulation) return;
    m_presentationNanoseconds = presentationNanoseconds;
    double alpha = 0;
    m_retainedFrame = m_simulation->retainFrameAt(presentationNanoseconds, alpha);
    loadFrame(*m_retainedFrame, m_simulation->palette(), alpha, m_simulation->config().deadly_walls);
    m_simulationTime += alpha * SnakeSimulation::physicsStepSeconds();
    update();
}

void SnakeRenderer::presentFrame(qreal simulationTime, qreal interpolation)
{
    m_simulationTime = simulationTime;
    m_interpolation = clamped(interpolation, 0.0, 1.0);
    update();
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

void SnakeRenderer::setDeveloperMode(bool enabled)
{
    if (m_developerMode == enabled) {
        return;
    }
    m_developerMode = enabled;
    update();
}

QSGNode *SnakeRenderer::updatePaintNode(QSGNode *oldNode,
                                        UpdatePaintNodeData *updatePaintNodeData)
{
    Q_UNUSED(updatePaintNodeData)
    // presentAt leases immutable history on the GUI thread. Later window
    // ticks cannot recycle this buffer before or after scene-graph sync.
    auto *node = static_cast<GeometryNode *>(oldNode);
    if (!node) {
        node = new GeometryNode;
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

    auto &vertices = node->vertices;
    vertices.clear();
    const auto snakes = m_frame ? std::span(m_frame->snakes) : std::span<const snakes_core_snake>();
    const auto food = m_frame ? std::span(m_frame->food) : std::span<const snakes_core_food>();
    qsizetype liveSegmentCount = 0;
    for (const auto &record : snakes) {
        const Snake snake(record, *m_frame);
        if (snake.alive) {
            liveSegmentCount += snake.segments.size();
        }
    }
    // Body ribbons, join discs, markings, eyes and crowns average fewer than
    // 42 vertices per segment. Reserving from actual segment count prevents
    // repeated CPU-side reallocations as a champion grows.
    const qsizetype estimatedVertices = food.size() * 72
        + liveSegmentCount * 42 + snakes.size() * 180;
    vertices.reserve(std::min<qsizetype>(estimatedVertices,
                                         std::numeric_limits<int>::max()));
    const QSizeF viewport(width(), height());
    const QPointF drawOffset(m_drawOffsetX, m_drawOffsetY);
    const qreal scaleX = m_scaleToViewport
        ? viewport.width() / std::max(1.0, m_worldWidth) : m_worldToViewX;
    const qreal scaleY = m_scaleToViewport
        ? viewport.height() / std::max(1.0, m_worldHeight) : m_worldToViewY;
    const qreal sizeScale = std::sqrt(scaleX * scaleY);
    const auto mapPoint = [drawOffset, scaleX, scaleY](const QPointF &point) {
        return QPointF(point.x() * scaleX + drawOffset.x(),
                       point.y() * scaleY + drawOffset.y());
    };

    // Avoid switching the complete food field between detail levels whenever
    // consumption and respawns hover around one exact particle count.
    if (m_denseFoodRendering) {
        m_denseFoodRendering = food.size() >= 280;
    } else {
        m_denseFoodRendering = food.size() > 340;
    }
    const bool denseFood = m_denseFoodRendering;
    for (const auto &record : food) {
        const Food particle(record);
        const qreal pulse = 0.82 + std::sin(m_simulationTime * 3.0 + particle.phase) * 0.18;
        const qreal worldSize = particle.size * pulse;
        const qreal size = worldSize * sizeScale;
        const QColor color = m_palette.isEmpty() ? QColor(Qt::cyan) : m_palette.at(particle.colorIndex % m_palette.size());
        Offsets xOffsets, yOffsets;
        if (!m_deadlyWalls) {
            xOffsets.extent = m_worldWidth;
            if (particle.position.x() < worldSize * 3.2) xOffsets.last = 1;
            if (particle.position.x() > m_worldWidth - worldSize * 3.2) xOffsets.first = -1;
            yOffsets.extent = m_worldHeight;
            if (particle.position.y() < worldSize * 3.2) yOffsets.last = 1;
            if (particle.position.y() > m_worldHeight - worldSize * 3.2) yOffsets.first = -1;
        }
        for (int xi = xOffsets.first; xi <= xOffsets.last; ++xi) {
            const qreal xOffset = xi * xOffsets.extent;
            for (int yi = yOffsets.first; yi <= yOffsets.last; ++yi) {
                const qreal yOffset = yi * yOffsets.extent;
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
                // At high density, retain the halo's apparent radius while
                // reducing only its tessellation. Removing the halo entirely
                // made every particle appear to shrink whenever a death burst
                // crossed the LOD threshold.
                appendDisc(vertices, center, size * 3.2,
                           withAlpha(color, 30), denseFood ? 6 : 8, viewport);
                appendDisc(vertices, center, size, withAlpha(color, 230),
                           denseFood ? 6 : 8, viewport);
                appendDisc(vertices, center - QPointF(size * 0.24, size * 0.24),
                           std::max(0.7, size * 0.3), QColor(255, 255, 255, 215),
                           denseFood ? 4 : 5, viewport);
            }
        }
    }

    size_t leaderLength = 0;
    for (const auto &record : snakes) {
        const Snake snake(record, *m_frame);
        if (snake.alive) {
            leaderLength = std::max(leaderLength, snake.segments.size());
        }
    }
    for (const auto &record : snakes) {
        const Snake snake(record, *m_frame);
        if (!snake.alive || snake.segments.size() < 2) {
            continue;
        }
        auto &points = node->points;
        points.clear();
        points.reserve(snake.segments.size());
        for (const Segment segment : snake.segments) {
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
            if (!points.empty() && !m_deadlyWalls) {
                x = points.back().x()
                    + axisDelta(wrapped(points.back().x(), m_worldWidth), x,
                                m_worldWidth, false);
                y = points.back().y()
                    + axisDelta(wrapped(points.back().y(), m_worldHeight), y,
                                m_worldHeight, false);
            }
            points.push_back(QPointF(x, y));
        }

        qreal minimumX = points.front().x();
        qreal maximumX = minimumX;
        qreal minimumY = points.front().y();
        qreal maximumY = minimumY;
        for (const QPointF &point : std::as_const(points)) {
            minimumX = std::min(minimumX, point.x());
            maximumX = std::max(maximumX, point.x());
            minimumY = std::min(minimumY, point.y());
            maximumY = std::max(maximumY, point.y());
        }
        Offsets xOffsets, yOffsets;
        if (!m_deadlyWalls) {
            const qreal margin = snake.radius * 3.0;
            xOffsets = wrappingOffsets(minimumX, maximumX, m_worldWidth, margin);
            yOffsets = wrappingOffsets(minimumY, maximumY, m_worldHeight, margin);
        }

        const QColor color = m_palette.isEmpty() ? QColor(Qt::cyan) : m_palette.at(snake.colorIndex % m_palette.size());
        const QColor outline(5, 7, 16, 175);
        const qreal radius = snake.radius * sizeScale;
        const qreal bodyMargin = radius * 1.4;
        for (int xi = xOffsets.first; xi <= xOffsets.last; ++xi) {
            const qreal xOffset = xi * xOffsets.extent;
            for (int yi = yOffsets.first; yi <= yOffsets.last; ++yi) {
                const qreal yOffset = yi * yOffsets.extent;
                const QPointF wrapOffset(xOffset, yOffset);
                const QRectF copyBounds(
                    (minimumX + xOffset) * scaleX + drawOffset.x() - bodyMargin,
                    (minimumY + yOffset) * scaleY + drawOffset.y() - bodyMargin,
                    (maximumX - minimumX) * scaleX + bodyMargin * 2.0,
                    (maximumY - minimumY) * scaleY + bodyMargin * 2.0);
                // In seamless mode most snakes are outside any one monitor.
                // Reject a whole wrapped copy before walking its body and
                // attempting to append/cull every individual primitive.
                if (!visible(copyBounds, viewport)) {
                    continue;
                }
                auto &renderedPoints = node->renderedPoints;
                renderedPoints.clear();
                renderedPoints.reserve(points.size());
                for (const QPointF &point : std::as_const(points)) {
                    renderedPoints.push_back(mapPoint(point + wrapOffset));
                }
                prepareRibbon(renderedPoints, node->normals, node->valid);
                appendRibbon(vertices, renderedPoints, node->normals, node->valid, radius * 1.275,
                             outline, viewport);
                appendDisc(vertices, renderedPoints.back(), radius * 1.275,
                           outline, 12, viewport);
                appendRibbon(vertices, renderedPoints, node->normals, node->valid, radius * 0.96,
                             withAlpha(color, 245), viewport);
                appendDisc(vertices, renderedPoints.back(), radius * 0.96,
                           withAlpha(color, 245), 12, viewport);
                for (size_t index = 5; index < points.size(); index += 6) {
                    appendDisc(vertices, renderedPoints[index], radius * 0.34,
                               QColor(255, 255, 255, 46), 6, viewport);
                }

                const QPointF head = renderedPoints.front();
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
                    appendDisc(vertices, eye, eyeRadius, QColor(Qt::white), 8, viewport);
                    appendDisc(vertices, eye + forward * (eyeRadius * 0.34),
                               eyeRadius * 0.48, QColor(17, 19, 26),
                               7, viewport);
                }
                if (points.size() == leaderLength) {
                    const QPointF crownCenter = head - forward * (radius * 0.32);
                    const std::array<QPointF, 7> crown{
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
                    const QColor gold(255, 216, 74);
                    for (size_t index = 0; index < crown.size(); ++index) {
                        appendTriangle(vertices, crownCenter, crown[index],
                                       crown[(index + 1) % crown.size()], gold);
                    }
                    for (size_t index = 0; index < crown.size(); ++index) {
                        appendSegment(vertices, crown[index],
                                      crown[(index + 1) % crown.size()],
                                      std::max(0.65, radius * 0.09),
                                      QColor(109, 67, 0), viewport);
                    }
                    appendDisc(vertices, crownCenter + forward * (radius * 0.04),
                               std::max(0.8, radius * 0.12),
                               QColor(255, 242, 160), 6, viewport);
                }
            }
        }
    }

    if (m_developerMode) {
        // The ABI exposes immediate steering, but no planned-route points.
        for (const auto &record : snakes) {
            const Snake snake(record, *m_frame);
            if (!snake.alive || snake.segments.empty()) continue;
            const Segment head(snake.segments.front());
            QPointF start(head.previous.x() + axisDelta(head.previous.x(), head.position.x(), m_worldWidth, m_deadlyWalls) * m_interpolation,
                          head.previous.y() + axisDelta(head.previous.y(), head.position.y(), m_worldHeight, m_deadlyWalls) * m_interpolation);
            if (!m_deadlyWalls) start = QPointF(wrapped(start.x(), m_worldWidth), wrapped(start.y(), m_worldHeight));
            const qreal length = std::max(52.0, snake.radius * 7.0);
            const Offsets xs = m_deadlyWalls ? Offsets{} : Offsets{-1, 1, m_worldWidth};
            const Offsets ys = m_deadlyWalls ? Offsets{} : Offsets{-1, 1, m_worldHeight};
            for (int xi = xs.first; xi <= xs.last; ++xi) {
                for (int yi = ys.first; yi <= ys.last; ++yi) {
                    const QPointF mapped = mapPoint(start + QPointF(xi * xs.extent, yi * ys.extent));
                    const QPointF end = mapped + QPointF(std::cos(snake.desiredAngle) * length * scaleX,
                                                        std::sin(snake.desiredAngle) * length * scaleY);
                    const QColor color(255, 255, 255, 235);
                    appendSegment(vertices, mapped, end, std::max(1.2, 1.9 * sizeScale), color, viewport);
                    const qreal angle = std::atan2(end.y() - mapped.y(), end.x() - mapped.x());
                    for (qreal side : {-0.62, 0.62}) {
                        const qreal wing = angle + 3.14159265358979323846 + side;
                        appendSegment(vertices, end, end + QPointF(std::cos(wing), std::sin(wing)) * std::max(8.0, 11.0 * sizeScale),
                                      std::max(1.2, 1.9 * sizeScale), color, viewport);
                    }
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
    if (!vertices.empty()) {
        std::copy(vertices.cbegin(), vertices.cend(), geometry->vertexDataAsColoredPoint2D());
    }
    geometry->markVertexDataDirty();
    node->markDirty(QSGNode::DirtyGeometry);
    return node;
}

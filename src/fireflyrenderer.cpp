// SPDX-License-Identifier: GPL-3.0-or-later
#include "fireflyrenderer.h"

#include <QColor>
#include <QPointF>
#include <QSGGeometry>
#include <QSGGeometryNode>
#include <QSGVertexColorMaterial>
#include <QVector>

#include <algorithm>
#include <cmath>
#include <cstring>

namespace {
using Vertex = QSGGeometry::ColoredPoint2D;

constexpr int circleSides = 12;
constexpr qreal tau = 6.28318530717958647692;

qreal fract(qreal value)
{
    return value - std::floor(value);
}

qreal stableRandom(int index, int seed)
{
    return fract(std::sin(index * 127.1 + seed * 311.7) * 43758.5453123);
}

qreal positiveModulo(qreal value, qreal modulus)
{
    if (modulus <= 0.0) {
        return 0.0;
    }
    value = std::fmod(value, modulus);
    return value < 0.0 ? value + modulus : value;
}

const QVector<QPointF> &unitCircle()
{
    static const QVector<QPointF> points = [] {
        QVector<QPointF> result;
        result.reserve(circleSides + 1);
        for (int side = 0; side <= circleSides; ++side) {
            const qreal angle = tau * side / circleSides;
            result.append(QPointF(std::cos(angle), std::sin(angle)));
        }
        return result;
    }();
    return points;
}

QVector<QColor> palette(const QString &name)
{
    if (name == QStringLiteral("spectrum")) {
        return {QColor("#ff477e"), QColor("#ffbe0b"), QColor("#42e2b8"),
                QColor("#3a86ff"), QColor("#b967ff"), QColor("#fb5607")};
    }
    if (name == QStringLiteral("ember")) {
        return {QColor("#fff1a8"), QColor("#ffc857"), QColor("#ff7b42"),
                QColor("#ef3e36"), QColor("#9c1c28"), QColor("#ffd6a5")};
    }
    if (name == QStringLiteral("forest")) {
        return {QColor("#d8f3dc"), QColor("#95d5b2"), QColor("#52b788"),
                QColor("#2d6a4f"), QColor("#b7e4c7"), QColor("#74c69d")};
    }
    if (name == QStringLiteral("mono")) {
        return {QColor("#ffffff"), QColor("#d9e1e8"), QColor("#aeb8c2"),
                QColor("#7f8b96"), QColor("#edf2f4"), QColor("#bac4ce")};
    }
    if (name == QStringLiteral("pastel")) {
        return {QColor("#ffc8dd"), QColor("#bde0fe"), QColor("#caffbf"),
                QColor("#ffd6a5"), QColor("#e7c6ff"), QColor("#a2d2ff")};
    }
    return {QColor("#d9fbff"), QColor("#3dd6e8"), QColor("#3a86ff"),
            QColor("#7358d6"), QColor("#2aa889"), QColor("#9bf6ff")};
}

void appendVertex(QVector<Vertex> &vertices, const QPointF &point,
                  const QColor &color, int alpha)
{
    const int clampedAlpha = std::clamp(alpha, 0, 255);
    const auto premultiplied = [clampedAlpha](int channel) {
        return uchar((channel * clampedAlpha + 127) / 255);
    };
    Vertex vertex;
    vertex.set(float(point.x()), float(point.y()), premultiplied(color.red()),
               premultiplied(color.green()), premultiplied(color.blue()),
               uchar(clampedAlpha));
    vertices.append(vertex);
}

void appendGlowDisc(QVector<Vertex> &vertices, const QPointF &center, qreal radius,
                    const QColor &color, int centerAlpha, int edgeAlpha)
{
    if (radius <= 0.0 || !std::isfinite(radius)
            || !std::isfinite(center.x()) || !std::isfinite(center.y())) {
        return;
    }
    const auto &circle = unitCircle();
    for (int side = 0; side < circleSides; ++side) {
        appendVertex(vertices, center, color, centerAlpha);
        appendVertex(vertices, center + circle[side] * radius, color, edgeAlpha);
        appendVertex(vertices, center + circle[side + 1] * radius, color, edgeAlpha);
    }
}
}

FireflyRenderer::FireflyRenderer(QQuickItem *parent)
    : QQuickItem(parent)
{
    setFlag(ItemHasContents, true);
}

void FireflyRenderer::configure(int density, qreal sizeScale, qreal motionSpeed,
                                qreal glowAmount, const QString &paletteName, int seed)
{
    m_density = std::clamp(density, 0, 100);
    m_sizeScale = std::clamp(sizeScale, 0.25, 4.0);
    m_motionSpeed = std::clamp(motionSpeed, 0.05, 10.0);
    m_glowAmount = std::clamp(glowAmount, 0.0, 1.0);
    m_paletteName = paletteName;
    m_seed = seed;
    update();
}

void FireflyRenderer::presentFrame(qreal phase)
{
    if (!std::isfinite(phase)) {
        return;
    }
    m_phase = phase;
    update();
}

QSGNode *FireflyRenderer::updatePaintNode(QSGNode *oldNode,
                                          UpdatePaintNodeData *)
{
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
    const int count = std::lround(12 + m_density * 0.75);
    // Two glow discs and one core disc, each made of 12 triangles.
    vertices.reserve(count * circleSides * 9);
    const QVector<QColor> colors = palette(m_paletteName);
    const qreal viewportWidth = width();
    const qreal viewportHeight = height();

    if (viewportWidth > 0.0 && viewportHeight > 0.0) {
        for (int index = 0; index < count; ++index) {
            const qreal speedFactor = 0.45 + stableRandom(index * 9 + 1, m_seed) * 0.9;
            const qreal time = m_phase * m_motionSpeed * speedFactor;
            const qreal centerX = positiveModulo(
                stableRandom(index * 9 + 2, m_seed) * viewportWidth
                    + std::sin(time + index * 2.13) * viewportWidth * 0.10
                    + time * viewportWidth * 0.015,
                viewportWidth);
            const qreal centerY = positiveModulo(
                stableRandom(index * 9 + 3, m_seed) * viewportHeight
                    + std::cos(time * 0.73 + index * 1.47) * viewportHeight * 0.12,
                viewportHeight);
            const qreal pulse = 0.45 + std::sin(time * 2.2 + index) * 0.35;
            const qreal coreRadius = (1.7 + stableRandom(index * 9 + 4, m_seed) * 3.2)
                * m_sizeScale;
            const qreal haloRadius = coreRadius * (3.5 + m_glowAmount * 8.0);
            const QColor color = colors[index % colors.size()];
            const QPointF center(centerX, centerY);

            appendGlowDisc(vertices, center, haloRadius, color,
                           std::lround(pulse * 34.0), 0);
            appendGlowDisc(vertices, center, haloRadius * 0.46, color,
                           std::lround(pulse * 78.0), 0);
            appendGlowDisc(vertices, center, coreRadius, QColor("#fffbd0"), 224, 150);
        }
    }

    QSGGeometry *geometry = node->geometry();
    if (geometry->vertexCount() != vertices.size()) {
        geometry->allocate(vertices.size());
    }
    if (!vertices.isEmpty()) {
        std::memcpy(geometry->vertexData(), vertices.constData(),
                    size_t(vertices.size()) * sizeof(Vertex));
    }
    node->markDirty(QSGNode::DirtyGeometry);
    return node;
}

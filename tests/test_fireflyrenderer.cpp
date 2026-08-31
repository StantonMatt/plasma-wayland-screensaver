// SPDX-License-Identifier: GPL-3.0-or-later
#include "fireflyrenderer.h"

#include <QSGGeometry>
#include <QSGGeometryNode>
#include <QTest>

#include <cmath>
#include <memory>

class FireflyRendererTest : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void createsOneBatchedGeometryNode()
    {
        FireflyRenderer renderer;
        renderer.setWidth(3440);
        renderer.setHeight(1440);
        renderer.configure(60, 1.0, 3.0, 0.75, QStringLiteral("ember"), 1);
        renderer.presentFrame(12.5);

        std::unique_ptr<QSGNode> node(renderer.updatePaintNode(nullptr, nullptr));
        auto *geometryNode = static_cast<QSGGeometryNode *>(node.get());
        QVERIFY(geometryNode);
        QVERIFY(geometryNode->geometry());

        constexpr int expectedFireflies = 57;
        constexpr int trianglesPerFirefly = 36;
        constexpr int verticesPerTriangle = 3;
        QCOMPARE(geometryNode->geometry()->vertexCount(),
                 expectedFireflies * trianglesPerFirefly * verticesPerTriangle);

        const auto *vertices = geometryNode->geometry()->vertexDataAsColoredPoint2D();
        for (int index = 0; index < geometryNode->geometry()->vertexCount(); ++index) {
            QVERIFY(std::isfinite(vertices[index].x));
            QVERIFY(std::isfinite(vertices[index].y));
            QVERIFY(vertices[index].r <= vertices[index].a);
            QVERIFY(vertices[index].g <= vertices[index].a);
            QVERIFY(vertices[index].b <= vertices[index].a);
            if (vertices[index].a == 0) {
                QCOMPARE(vertices[index].r, 0);
                QCOMPARE(vertices[index].g, 0);
                QCOMPARE(vertices[index].b, 0);
            }
        }
    }

    void reusesGeometryAcrossFrames()
    {
        FireflyRenderer renderer;
        renderer.setWidth(2560);
        renderer.setHeight(1440);
        renderer.configure(50, 1.0, 1.0, 0.35, QStringLiteral("ocean"), 7);

        QSGNode *node = renderer.updatePaintNode(nullptr, nullptr);
        auto *geometryNode = static_cast<QSGGeometryNode *>(node);
        QSGGeometry *geometry = geometryNode->geometry();
        renderer.presentFrame(4000.0);
        QSGNode *updatedNode = renderer.updatePaintNode(node, nullptr);

        QCOMPARE(updatedNode, node);
        QCOMPARE(static_cast<QSGGeometryNode *>(updatedNode)->geometry(), geometry);
        delete updatedNode;
    }

    void benchmarkThreeMonitorFrameAssembly()
    {
        FireflyRenderer renderers[3];
        const QSizeF sizes[] = {{2560, 1440}, {3440, 1440}, {1920, 1080}};
        QSGNode *nodes[3] = {};
        for (int index = 0; index < 3; ++index) {
            renderers[index].setSize(sizes[index]);
            renderers[index].configure(60, 1.0, 3.0, 0.75,
                                       QStringLiteral("ember"), 1);
            nodes[index] = renderers[index].updatePaintNode(nullptr, nullptr);
        }

        qreal phase = 10.0;
        QBENCHMARK {
            phase += 1.0 / 60.0;
            for (int index = 0; index < 3; ++index) {
                renderers[index].presentFrame(phase);
                nodes[index] = renderers[index].updatePaintNode(nodes[index], nullptr);
            }
        }

        for (QSGNode *node : nodes) {
            delete node;
        }
    }
};

QTEST_MAIN(FireflyRendererTest)
#include "test_fireflyrenderer.moc"

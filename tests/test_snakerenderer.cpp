// SPDX-License-Identifier: GPL-3.0-or-later
#include "snakerenderer.h"

#include <QJSEngine>
#include <QSGGeometryNode>
#include <QTest>

#include <cmath>
#include <limits>

class SnakeRendererTest final : public QObject
{
    Q_OBJECT

private:
    static QJSValue makeSnakes(QJSEngine &engine, int segmentCount,
                               bool malformedSegment = false, qreal coordinateScale = 1.0)
    {
        QJSValue snakes = engine.newArray(1);
        QJSValue snake = engine.newObject();
        snake.setProperty(QStringLiteral("alive"), true);
        snake.setProperty(QStringLiteral("radius"), 8.0);
        snake.setProperty(QStringLiteral("angle"), 0.0);
        snake.setProperty(QStringLiteral("colorIndex"), 0);

        QJSValue segments = engine.newArray(segmentCount);
        for (int index = 0; index < segmentCount; ++index) {
            QJSValue segment = engine.newObject();
            const qreal x = (250.0 - index * 4.5) * coordinateScale;
            const qreal y = (120.0 + std::sin(index * 0.08) * 18.0) * coordinateScale;
            segment.setProperty(QStringLiteral("x"),
                                malformedSegment && index == 7
                                    ? std::numeric_limits<qreal>::quiet_NaN() : x);
            segment.setProperty(QStringLiteral("y"), y);
            segment.setProperty(QStringLiteral("previousX"), x - 1.0);
            segment.setProperty(QStringLiteral("previousY"), y);
            segments.setProperty(index, segment);
        }
        snake.setProperty(QStringLiteral("segments"), segments);
        snakes.setProperty(0, snake);
        return snakes;
    }

    static QJSValue makeFood(QJSEngine &engine)
    {
        QJSValue food = engine.newArray(1);
        QJSValue particle = engine.newObject();
        particle.setProperty(QStringLiteral("x"), 120.0);
        particle.setProperty(QStringLiteral("y"), 70.0);
        particle.setProperty(QStringLiteral("size"), 4.0);
        particle.setProperty(QStringLiteral("phase"), 0.0);
        particle.setProperty(QStringLiteral("attraction"), 0.0);
        particle.setProperty(QStringLiteral("attractionX"), 120.0);
        particle.setProperty(QStringLiteral("attractionY"), 70.0);
        particle.setProperty(QStringLiteral("colorIndex"), 0);
        food.setProperty(0, particle);
        return food;
    }

private Q_SLOTS:
    void retainedGeometrySurvivesGrowthShrinkAndMalformedPrimitive()
    {
        SnakeRenderer renderer;
        renderer.setSize(QSizeF(320, 240));

        QJSEngine engine;
        QJSValue palette = engine.newArray(1);
        palette.setProperty(0, QStringLiteral("#4de6ff"));
        const QJSValue food = makeFood(engine);

        QSGNode *node = nullptr;
        const auto buildGeometry = [&](int segmentCount, bool malformed = false) {
            renderer.syncFrame(makeSnakes(engine, segmentCount, malformed), food, palette,
                               segmentCount / 30.0, 0.5, 320, 240, 0, 0, true);
            node = renderer.updatePaintNode(node, nullptr);
            return static_cast<QSGGeometryNode *>(node)->geometry()->vertexCount();
        };

        const int initialVertices = buildGeometry(24);
        QVERIFY(initialVertices > 250);
        QVERIFY(renderer.m_geometryCapacity >= initialVertices);

        const int grownVertices = buildGeometry(420);
        QVERIFY(grownVertices > initialVertices);
        const int grownCapacity = renderer.m_geometryCapacity;
        QVERIFY(grownCapacity >= grownVertices);

        const int shrunkVertices = buildGeometry(16);
        QVERIFY(shrunkVertices < grownVertices);
#if QT_VERSION >= QT_VERSION_CHECK(6, 10, 0)
        QCOMPARE(renderer.m_geometryCapacity, grownCapacity);
#else
        QCOMPARE(renderer.m_geometryCapacity, shrunkVertices);
#endif

        const int guardedVertices = buildGeometry(80, true);
        QVERIFY(guardedVertices > 250);
        QCOMPARE(guardedVertices % 3, 0);
        delete node;
    }

    void sameTimestampResyncUsesRescaledPreviousPositions()
    {
        SnakeRenderer renderer;
        QJSEngine engine;
        const QJSValue empty = engine.newArray();

        renderer.syncFrame(makeSnakes(engine, 12), empty, empty,
                           4.0, 0.5, 320, 240, 0, 0, true);
        renderer.syncFrame(makeSnakes(engine, 12, false, 2.0), empty, empty,
                           4.0, 0.5, 640, 480, 0, 0, true);

        QCOMPARE(renderer.m_snakes.size(), 1);
        QCOMPARE(renderer.m_snakes[0].segments.size(), 12);
        QCOMPARE(renderer.m_snakes[0].segments[0].position, QPointF(500.0, 240.0));
        QCOMPARE(renderer.m_snakes[0].segments[0].previous, QPointF(499.0, 240.0));
    }
};

QTEST_MAIN(SnakeRendererTest)
#include "test_snakerenderer.moc"

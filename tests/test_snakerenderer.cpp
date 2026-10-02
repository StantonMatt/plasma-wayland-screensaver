// SPDX-License-Identifier: GPL-3.0-or-later
#include "snakerenderer.h"
#include <QSGGeometryNode>
#include <QTest>
#include <cmath>
#include <limits>

class SnakeRendererTest final : public QObject
{
    Q_OBJECT
    static SnakeFrame makeFrame(int segments, int snakeCount = 1, int foodCount = 1,
                                bool malformed = false, float coordinateScale = 1)
    {
        SnakeFrame frame;
        frame.info = {600, 20, 3440, 1440, 0};
        for (int n = 0; n < snakeCount; ++n) {
            frame.snakes.push_back({uint32_t(n), 1, 1, 0, 8, 0, 0.7,
                                   uint32_t(frame.segments.size()), uint32_t(segments)});
            for (int i = 0; i < segments; ++i) {
                const float x = (250 - i * 4.5 + (n % 4) * 780) * coordinateScale;
                const float y = (120 + std::sin(i * 0.08) * 18 + (n / 4) * 340) * coordinateScale;
                frame.segments.push_back({malformed && i == 7 ? std::numeric_limits<float>::quiet_NaN() : x,
                                          y, x - 1, y});
            }
        }
        for (int i = 0; i < foodCount; ++i) {
            const float x = 12 + (i % 32) * 9;
            const float y = 12 + (i / 32) * 9;
            frame.food.push_back({uint64_t(i + 1), x, y, 4, 0, 0, x, y, 0});
        }
        return frame;
    }
    const QVector<QColor> palette{QColor("#4de6ff")};
private Q_SLOTS:
    void retainedGeometrySurvivesGrowthShrinkAndMalformedPrimitive()
    {
        SnakeRenderer renderer;
        renderer.setSize(QSizeF(320, 240));
        SnakeFrame frame;
        QSGNode *node = nullptr;
        const auto render = [&](int count, bool malformed = false) {
            frame = makeFrame(count, 1, 1, malformed);
            frame.info.world_width = 320; frame.info.world_height = 240;
            renderer.syncFrame(frame, palette, 0.5, true);
            node = renderer.updatePaintNode(node, nullptr);
            return static_cast<QSGGeometryNode *>(node)->geometry()->vertexCount();
        };
        const int initial = render(24);
        QVERIFY(initial > 250);
        const int grown = render(420);
        QVERIFY(grown > initial);
        const int capacity = renderer.m_geometryCapacity;
        const int shrunk = render(16);
        QVERIFY(shrunk < grown);
#if QT_VERSION >= QT_VERSION_CHECK(6, 10, 0)
        QCOMPARE(renderer.m_geometryCapacity, capacity);
#else
        QCOMPARE(capacity, grown);
        QCOMPARE(renderer.m_geometryCapacity, shrunk);
#endif
        const int guarded = render(80, true);
        QVERIFY(guarded > 250);
        QCOMPARE(guarded % 3, 0);
        delete node;
    }
    void resyncReadsNativePreviousPositions()
    {
        SnakeRenderer renderer;
        auto frame = makeFrame(12);
        renderer.syncFrame(frame, palette, 0.5, true);
        auto scaled = makeFrame(12, 1, 1, false, 2);
        renderer.syncFrame(scaled, palette, 0.5, true);
        QCOMPARE(renderer.m_frame, &scaled);
        QCOMPARE(renderer.m_frame->segments[0].x, 500.0f);
        QCOMPARE(renderer.m_frame->segments[0].previous_x, 499.0f);
    }
    void denseFoodDetailUsesHysteresis()
    {
        SnakeRenderer renderer;
        renderer.setSize(QSizeF(320, 240));
        QSGNode *node = nullptr;
        SnakeFrame frame;
        const auto render = [&](int count) {
            frame = makeFrame(0, 0, count);
            renderer.syncFrame(frame, palette, 0, true);
            node = renderer.updatePaintNode(node, nullptr);
        };
        render(341); QVERIFY(renderer.m_denseFoodRendering);
        render(300); QVERIFY(renderer.m_denseFoodRendering);
        render(279); QVERIFY(!renderer.m_denseFoodRendering);
        delete node;
    }
    void developerSteeringArrowAndViewportScaling()
    {
        SnakeRenderer renderer;
        renderer.setSize(QSizeF(320, 240));
        auto frame = makeFrame(18);
        frame.info.world_width = 640; frame.info.world_height = 480;
        renderer.setScaleToViewport(true);
        renderer.syncFrame(frame, palette, 0.5, false);
        QSGNode *node = renderer.updatePaintNode(nullptr, nullptr);
        const int normal = static_cast<QSGGeometryNode *>(node)->geometry()->vertexCount();
        renderer.setDeveloperMode(true);
        node = renderer.updatePaintNode(node, nullptr);
        QVERIFY(static_cast<QSGGeometryNode *>(node)->geometry()->vertexCount() > normal);
        delete node;
    }
    void sharedNativeFrameAndDestruction()
    {
        auto *simulation = new SnakeSimulation({640, 480, 50, 35, 100, 100, 75, 1, 6, 0, 1});
        SnakeRenderer a, b;
        a.setSimulation(simulation); b.setSimulation(simulation);
        simulation->advance(1.0 / 30);
        QCOMPARE(a.m_frame, &simulation->frame());
        QCOMPARE(b.m_frame, a.m_frame);
        delete simulation;
        QVERIFY(!a.m_frame); QVERIFY(!b.m_frame);
        QVERIFY(!a.simulation()); QVERIFY(!b.simulation());
    }
    void abiLimitedArenaMapsToEveryViewport_data()
    {
        QTest::addColumn<QString>("behavior");
        QTest::addColumn<QSizeF>("extent");
        QTest::addColumn<bool>("deadly");
        for (const QString &mode : {QStringLiteral("independent"), QStringLiteral("synchronized"), QStringLiteral("seamless")}) {
            for (const QSizeF &extent : {QSizeF(24000, 1200), QSizeF(1600, 24000), QSizeF(24000, 20000)}) {
                for (bool deadly : {true, false}) {
                    const auto name = QStringLiteral("%1-%2x%3-%4").arg(mode).arg(extent.width()).arg(extent.height()).arg(deadly);
                    QTest::newRow(qPrintable(name)) << mode << extent << deadly;
                }
            }
        }
    }
    void abiLimitedArenaMapsToEveryViewport()
    {
        QFETCH(QString, behavior); QFETCH(QSizeF, extent); QFETCH(bool, deadly);
        SnakeSimulation simulation({1280, 720, 50, 35, 100, 100, 75, 1, 6, 0, uint32_t(deadly)});
        QVERIFY(simulation.resize(extent.width(), extent.height()));
        const auto &config = simulation.config();
        SnakeRenderer renderer;
        renderer.setSimulation(&simulation);
        const QSizeF viewport = behavior == QStringLiteral("independent") ? extent : QSizeF(800, 600);
        renderer.setSize(viewport);
        renderer.setScaleToViewport(behavior == QStringLiteral("synchronized"));
        const QPointF offset = behavior == QStringLiteral("seamless")
            ? QPointF(viewport.width() - extent.width(), viewport.height() - extent.height()) : QPointF();
        renderer.setDrawOffset(offset.x(), offset.y());
        // Put both food and a snake near the far desktop edge, beyond 16384
        // logical pixels. They must remain visible on that edge's viewport.
        const QPointF logical(extent.width() - 400, extent.height() - 300);
        const float x = logical.x() * config.width / extent.width();
        const float y = logical.y() * config.height / extent.height();
        SnakeFrame frame;
        frame.info = {0, 0, config.width, config.height, 0};
        frame.food.push_back({1, x, y, 3, 0, 0, x, y, 0});
        frame.snakes.push_back({0, 1, 1, 0, 6, 0, 0, 0, 2});
        frame.segments.push_back({x, y, x, y});
        frame.segments.push_back({x - 12, y, x - 12, y});
        renderer.syncFrame(frame, palette, 0, deadly);
        QSGNode *node = renderer.updatePaintNode(nullptr, nullptr);
        auto *geometry = static_cast<QSGGeometryNode *>(node)->geometry();
        QVERIFY(geometry->vertexCount() > 63); // Food plus the culled-copy snake path.
        const auto &center = geometry->vertexDataAsColoredPoint2D()[0];
        const QPointF expected = behavior == QStringLiteral("synchronized")
            ? QPointF(logical.x() * viewport.width() / extent.width(), logical.y() * viewport.height() / extent.height())
            : logical + offset;
        QVERIFY(std::abs(center.x - expected.x()) < 0.01);
        QVERIFY(std::abs(center.y - expected.y()) < 0.01);
        const int normal = geometry->vertexCount();
        renderer.setDeveloperMode(true);
        node = renderer.updatePaintNode(node, nullptr);
        QVERIFY(geometry->vertexCount() > normal);
        delete node;
    }
    void benchmarkMatureGeometry()
    {
        SnakeRenderer renderer;
        renderer.setSize(QSizeF(3440, 1440));
        auto frame = makeFrame(120, 14, 400);
        renderer.syncFrame(frame, palette, 0.5, true);
        QSGNode *node = renderer.updatePaintNode(nullptr, nullptr);
        QBENCHMARK {
            renderer.presentFrame(20, 0.5);
            node = renderer.updatePaintNode(node, nullptr);
        }
        delete node;
    }
    void benchmarkMatureSyncFrame()
    {
        SnakeRenderer renderer;
        renderer.setSize(QSizeF(3440, 1440));
        auto frame = makeFrame(120, 14, 400);
        QBENCHMARK {
            frame.info.simulation_time += 1.0 / 30;
            renderer.syncFrame(frame, palette, 0.5, true);
        }
    }
};
QTEST_MAIN(SnakeRendererTest)
#include "test_snakerenderer.moc"

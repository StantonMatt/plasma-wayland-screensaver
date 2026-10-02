// SPDX-License-Identifier: GPL-3.0-or-later
#include "snakerenderer.h"
#include "configuration.h"
#include <QTemporaryDir>
#include <QSignalSpy>
#include <QSGGeometryNode>
#include <QTest>
#include <QElapsedTimer>
#include <QCryptographicHash>
#include <numeric>
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
    void perWindowPredictionSurvivesAnotherWindowStepping()
    {
        const snakes_core_config config{320, 240, 50, 35, 100, 100, 75, 1, 6, 0, 1};
        SnakeSimulation sim(config);
        SnakeRenderer first, second;
        first.setSize(QSizeF(320, 240)); second.setSize(QSizeF(320, 240));
        first.setSimulation(&sim); second.setSimulation(&sim);
        constexpr qint64 origin = 1'000'000'000;
        sim.advanceTo(origin);
        sim.advanceTo(origin + 30'000'000);
        first.presentAt(origin + 30'000'000);
        // Cross a physics boundary before the first window's scene-graph sync.
        sim.advanceTo(origin + 40'000'000);
        second.presentAt(origin + 40'000'000);
        auto *firstNode = first.updatePaintNode(nullptr, nullptr);
        auto *secondNode = second.updatePaintNode(nullptr, nullptr);
        QCOMPARE(first.m_frame->info.tick, 0U);
        QCOMPARE(second.m_frame->info.tick, 1U);
        QVERIFY(std::abs(first.m_interpolation - 0.9) < 1e-9);
        QVERIFY(std::abs(second.m_interpolation - 0.2) < 1e-9);
        QVERIFY(std::abs(first.m_simulationTime - 0.03) < 1e-9);
        QVERIFY(std::abs(second.m_simulationTime - 0.04) < 1e-9);
        delete firstNode; delete secondNode;
    }

    void pendingPresentationSurvivesHistoryRecycling()
    {
        SnakeSimulation sim({320, 240, 50, 35, 100, 100, 75, 1, 6, 0, 1});
        SnakeRenderer view;
        view.setSize(QSizeF(320, 240)); view.setSimulation(&sim);
        constexpr qint64 origin = 1'000'000'000;
        sim.advanceTo(origin);
        sim.advanceTo(origin + 30'000'000);
        view.presentAt(origin + 30'000'000);
        // A window can wait arbitrarily long for scene-graph sync. Its pending
        // sample must survive even when every history buffer has been recycled.
        for (int i = 1; i <= 100; ++i) sim.advanceTo(origin + i * 33'333'334LL);
        auto *node = view.updatePaintNode(nullptr, nullptr);
        QCOMPARE(view.m_frame->info.tick, 0U);
        QVERIFY(std::abs(view.m_simulationTime - 0.03) < 1e-9);
        delete node;
    }

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
    void pausedSharedStateReachesEveryRenderer()
    {
        SnakeSimulation simulation({640, 480, 50, 35, 100, 100, 75, 1, 6, 0, 1});
        simulation.setPaused(true);
        SnakeRenderer first, second;
        first.setSize(QSizeF(320, 240)); second.setSize(QSizeF(320, 240));
        first.setSimulation(&simulation); second.setSimulation(&simulation);
        first.setDeveloperMode(true); second.setDeveloperMode(true);
        QSignalSpy updates(&simulation, &SnakeSimulation::presented);
        QVERIFY(simulation.resize(800, 600));
        QCOMPARE(updates.count(), 1);
        for (auto *renderer : {&first, &second}) {
            QCOMPARE(renderer->m_frame, &simulation.frame());
            QCOMPARE(renderer->m_frame->info.world_width, 800);
            QCOMPARE(renderer->m_frame->info.world_height, 600);
        }
        QTemporaryDir dir;
        Configuration settings(dir.filePath(QStringLiteral("settingsrc")));
        settings.setReducedMotion(true);
        settings.setAnimationPalette(QStringLiteral("ember"));
        simulation.applySettings(settings);
        QCOMPARE(updates.count(), 2);
        QCOMPARE(simulation.frame().info.tick, 0U);
        for (auto *renderer : {&first, &second}) {
            QCOMPARE(renderer->m_palette, SnakeSimulation::colors(QStringLiteral("ember")));
            QCOMPARE(renderer->m_frame, &simulation.frame());
            auto *node = renderer->updatePaintNode(nullptr, nullptr);
            QVERIFY(static_cast<QSGGeometryNode *>(node)->geometry()->vertexCount() > 0);
            delete node;
        }
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
    void geometryFingerprint_data()
    {
        QTest::addColumn<bool>("deadly");
        QTest::addColumn<bool>("developer");
        QTest::addColumn<double>("alpha");
        QTest::addColumn<QByteArray>("expected");
        QTest::newRow("walls") << true << false << 0.5 << QByteArray("55e68e536a65e2ceb3d5c1258b9a887ef679ae972608acc65bdd7c7c7dc691af");
        QTest::newRow("wrapping") << false << false << 0.9 << QByteArray("3c7f9798331487979dc67386d05e72b9a0a49737525cac1e1289f449243e7b03");
        QTest::newRow("developer") << true << true << 0.1 << QByteArray("a7558b4dbf84d518ba4f4de41d104643c2a11a936c7667945212818e05b81ee0");
    }
    void geometryFingerprint()
    {
        QFETCH(bool,deadly);QFETCH(bool,developer);QFETCH(double,alpha);
        QFETCH(QByteArray,expected);
        SnakeRenderer renderer;
        renderer.setSize(QSizeF(3440,1440));
        renderer.setDeveloperMode(developer);
        auto frame=makeFrame(120,14,400,true);
        renderer.syncFrame(frame,palette,alpha,deadly);
        auto *node=static_cast<QSGGeometryNode *>(renderer.updatePaintNode(nullptr,nullptr));
        auto *g=node->geometry();
        const auto bytes=QByteArrayView(reinterpret_cast<const char *>(g->vertexData()),
            g->vertexCount()*g->sizeOfVertex());
        const auto hash=QCryptographicHash::hash(bytes,QCryptographicHash::Sha256).toHex();
        qInfo() << "geometry" << QTest::currentDataTag() << g->vertexCount() << hash;
        QCOMPARE(hash,expected);
        delete node;
    }
    void benchmarkEcosystemPhases_data()
    {
        QTest::addColumn<bool>("deadly");
        QTest::newRow("walls") << true;
        QTest::newRow("wrapping") << false;
    }
    void benchmarkEcosystemPhases()
    {
        QFETCH(bool,deadly);
        snakes_core_config config{3440,1440,100,100,100,100,100,20260814,6,1,uint32_t(deadly)};
        snakes_core_world *handle=nullptr;
        QCOMPARE(snakes_core_create(&config,&handle),SNAKES_CORE_OK);
        std::unique_ptr<snakes_core_world,decltype(&snakes_core_destroy)> world(handle,snakes_core_destroy);
        QVERIFY(world);
        QCOMPARE(snakes_core_step(world.get(),6*1800),SNAKES_CORE_OK);
        SnakeRenderer first,second;
        first.setSize(QSizeF(3440,1440));second.setSize(QSizeF(3440,1440));
        SnakeFrame frame;
        QSGNode *nodes[2]{};
        std::array<std::vector<double>,4> times;
        for (auto &sample:times) sample.reserve(1800);
        uint64_t vertices=0,segments=0,foods=0;
        QElapsedTimer timer;
        for (int tick=0;tick<1800;++tick) {
            timer.start();
            QCOMPARE(snakes_core_step(world.get(),1),SNAKES_CORE_OK);
            times[0].push_back(timer.nsecsElapsed()/1e6);
            timer.restart();
            snakes_core_frame_sizes sizes{};
            QCOMPARE(snakes_core_get_frame_sizes(world.get(),&sizes),SNAKES_CORE_OK);
            frame.snakes.resize(sizes.snakes);frame.segments.resize(sizes.segments);frame.food.resize(sizes.food);
            QCOMPARE(snakes_core_export_frame(world.get(),frame.snakes.data(),frame.snakes.size(),
                frame.segments.data(),frame.segments.size(),frame.food.data(),frame.food.size(),&frame.info),SNAKES_CORE_OK);
            times[1].push_back(timer.nsecsElapsed()/1e6);
            segments+=sizes.segments;foods+=sizes.food;
            int index=0;
            for (auto *view:{&first,&second}) {
                timer.restart();
                view->syncFrame(frame,palette,0.5,deadly);
                nodes[index]=view->updatePaintNode(nodes[index],nullptr);
                times[2+index].push_back(timer.nsecsElapsed()/1e6);
                if (!index) vertices+=static_cast<QSGGeometryNode *>(nodes[index])->geometry()->vertexCount();
                ++index;
            }
        }
        const char *names[]{"step_ms","export_ms","first_window_ms","second_window_ms"};
        for (size_t i=0;i<times.size();++i) {
            auto &sample=times[i];const double mean=std::accumulate(sample.begin(),sample.end(),0.0)/sample.size();
            std::sort(sample.begin(),sample.end());
            qInfo() << names[i] << "mean" << mean << "p50" << sample[900] << "p95" << sample[1710];
        }
        qInfo() << "mean_segments" << segments/1800.0 << "mean_food" << foods/1800.0 << "mean_vertices" << vertices/1800.0;
        delete nodes[0];delete nodes[1];
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

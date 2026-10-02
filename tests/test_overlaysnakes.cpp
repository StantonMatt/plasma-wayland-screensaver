// SPDX-License-Identifier: GPL-3.0-or-later
#include "overlaymanager.h"
#include "snakesimulation.h"
#include "snakerenderer.h"
#include "configuration.h"
#include <QGuiApplication>
#include <QQuickView>
#include <QScreen>
#include <QTemporaryDir>
#include <QTest>

class OverlaySnakesTest final : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void worldFollowsOverlayConfigure_data()
    {
        QTest::addColumn<QString>("behavior");
        QTest::newRow("independent") << QStringLiteral("independent");
        QTest::newRow("synchronized") << QStringLiteral("synchronized");
        QTest::newRow("seamless") << QStringLiteral("seamless");
    }
    void worldFollowsOverlayConfigure()
    {
        QFETCH(QString, behavior);
        QTemporaryDir dir;
        Configuration settings(dir.filePath(QStringLiteral("settingsrc")));
        settings.setVisualModule(QStringLiteral("snakes"));
        settings.setMonitorBehavior(behavior);
        settings.setCoverPanels(false);
        settings.setReducedMotion(true);
        settings.setShowClock(false);
        OverlayManager manager(&settings);
        QVERIFY(manager.show());
        auto *screen = QGuiApplication::primaryScreen();
        auto *view = manager.m_views.value(screen);
        auto *world = manager.m_snakeSimulations.value(screen);
        QVERIFY(view); QVERIFY(world);
        // Model a compositor configure leaving room for panels: QScreen is
        // unchanged while both the window's origin and viewport change.
        const QRect viewport(-100, 60, 620, 420);
        view->setGeometry(viewport);
        QTRY_COMPARE(world->viewSize(), QSizeF(viewport.size()));
        QCOMPARE(world->config().width, 620);
        QCOMPARE(world->config().height, 420);
        world->setPaused(false);
        world->advance(1.0 / 30);
        const auto tick = world->frame().info.tick;
        view->resize(600, 400);
        QTRY_COMPARE(world->viewSize(), QSizeF(600, 400));
        QCOMPARE(world->frame().info.tick, tick);
        // Settings and geometry refreshes must preserve configured dimensions.
        settings.setAnimationSpeed(125);
        settings.setCoverPanels(true);
        manager.updateAllViewGeometry();
        QCOMPARE(view->size(), QSize(600, 400));
        QCOMPARE(world->viewSize(), QSizeF(600, 400));
        QCOMPARE(world->config().speed, 125);
        for (const QString &mode : {QStringLiteral("independent"), QStringLiteral("synchronized"), QStringLiteral("seamless")}) {
            settings.setMonitorBehavior(mode);
            world = manager.m_snakeSimulations.value(screen);
            QVERIFY(world);
            QCOMPARE(world->viewSize(), QSizeF(600, 400));
            auto *renderer = manager.m_snakeRenderers.value(screen);
            QCOMPARE(renderer->drawOffsetX(), 0);
            QCOMPARE(renderer->drawOffsetY(), 0);
        }
        manager.hide();
        QCoreApplication::sendPostedEvents(nullptr, QEvent::DeferredDelete);
    }
    void sharedArenaUsesAllOverlayViewports_data()
    {
        QTest::addColumn<QString>("behavior");
        QTest::newRow("synchronized") << QStringLiteral("synchronized");
        QTest::newRow("seamless") << QStringLiteral("seamless");
    }
    void sharedArenaUsesAllOverlayViewports()
    {
        QFETCH(QString, behavior);
        QTemporaryDir dir;
        Configuration settings(dir.filePath(QStringLiteral("settingsrc")));
        settings.setVisualModule(QStringLiteral("snakes"));
        settings.setMonitorBehavior(behavior);
        settings.setReducedMotion(true);
        settings.setShowClock(false);
        OverlayManager manager(&settings);
        QVERIFY(manager.show());
        auto *screen = QGuiApplication::primaryScreen();
        auto *first = manager.m_views.value(screen);
        first->setGeometry(-600, 40, 600, 400);
        // A synthetic second output makes hotplug/offset tests independent of
        // the test machine's monitor count. It does not create a native surface.
        auto *second = new QQuickView;
        second->setGeometry(0, 70, 19000, 350);
        auto *renderer = new SnakeRenderer(second->contentItem());
        manager.m_views.insert(nullptr, second);
        manager.m_snakeRenderers.insert(nullptr, renderer);
        manager.configureSnakeRenderSharing();
        auto *world = manager.m_sharedSnakeSimulation.get();
        QVERIFY(world);
        QCOMPARE(manager.m_snakeSimulations.value(nullptr), world);
        const bool seamless = behavior == QStringLiteral("seamless");
        QCOMPARE(world->viewSize(), seamless ? QSizeF(19600, 400) : QSizeF(600, 400));
        QCOMPARE(world->config().width, seamless ? 16384 : 600);
        QCOMPARE(renderer->drawOffsetX(), seamless ? -600 : 0);
        QCOMPARE(renderer->drawOffsetY(), seamless ? -30 : 0);
        // Resize and move a non-driver output, then remove the driver. Shared
        // state survives and the replacement arena is the surviving viewport.
        second->setGeometry(100, 80, 20000, 300);
        manager.configureSnakeRenderSharing();
        QCOMPARE(world->viewSize(), seamless ? QSizeF(20700, 400) : QSizeF(600, 400));
        world->setPaused(false); world->advance(1.0 / 30);
        manager.removeScreen(screen);
        if (!seamless) {
            // nullptr is not a real screen, so exercise the real-screen
            // hotplug path by reattaching the existing output below instead.
            QVERIFY(manager.addScreen(screen));
            first = manager.m_views.value(screen);
            first->setGeometry(0, 0, 500, 300);
            manager.configureSnakeRenderSharing();
            QCOMPARE(world->viewSize(), QSizeF(500, 300));
        } else {
            QCOMPARE(world->viewSize(), QSizeF(20000, 300));
            QCOMPARE(renderer->drawOffsetX(), 0);
            QCOMPARE(renderer->drawOffsetY(), 0);
        }
        QCOMPARE(world->frame().info.tick, 1U);
        manager.hide();
        QCoreApplication::sendPostedEvents(nullptr, QEvent::DeferredDelete);
    }
    void sharedWorldSurvivesDriverRemoval_data()
    {
        QTest::addColumn<QString>("behavior");
        QTest::newRow("synchronized") << QStringLiteral("synchronized");
        QTest::newRow("seamless") << QStringLiteral("seamless");
    }
    void sharedWorldSurvivesDriverRemoval()
    {
        QFETCH(QString, behavior);
        QTemporaryDir dir;
        Configuration settings(dir.filePath(QStringLiteral("settingsrc")));
        settings.setVisualModule(QStringLiteral("snakes"));
        settings.setMonitorBehavior(behavior);
        settings.setReducedMotion(true);
        settings.setShowClock(false);
        OverlayManager manager(&settings);
        QVERIFY(manager.show());
        auto *screen = QGuiApplication::primaryScreen();
        auto *world = manager.m_sharedSnakeSimulation.get();
        QVERIFY(world);
        QCOMPARE(manager.m_snakeSimulations.value(screen), world);
        world->setPaused(false);
        manager.advanceSnakeSimulation(screen, 1.0 / 30);
        QCOMPARE(world->frame().info.tick, 1U);
        // A second viewport shares the same world but is not the clock driver.
        manager.m_snakeSimulations.insert(nullptr, world);
        manager.advanceSnakeSimulation(nullptr, 1.0 / 30);
        QCOMPARE(world->frame().info.tick, 1U);
        manager.m_snakeSimulations.remove(nullptr);
        manager.removeScreen(screen);
        QCOMPARE(manager.m_sharedSnakeSimulation.get(), world);
        QCOMPARE(world->frame().info.tick, 1U);
        QVERIFY(manager.addScreen(screen));
        QCOMPARE(manager.m_snakeSimulations.value(screen), world);
        QCOMPARE(world->frame().info.tick, 1U);
        const QPointer<SnakeSimulation> lifetime(world);
        manager.hide();
        QVERIFY(!lifetime);
        QCoreApplication::sendPostedEvents(nullptr, QEvent::DeferredDelete);
        QCOMPARE(manager.m_pendingViewDeletions, 0);
    }
    void independentWorldDestroyedWithView()
    {
        QTemporaryDir dir;
        Configuration settings(dir.filePath(QStringLiteral("settingsrc")));
        settings.setVisualModule(QStringLiteral("snakes"));
        settings.setMonitorBehavior(QStringLiteral("independent"));
        settings.setReducedMotion(true);
        OverlayManager manager(&settings);
        QVERIFY(manager.show());
        QVERIFY(!manager.m_sharedSnakeSimulation);
        const QPointer<SnakeSimulation> world(manager.m_snakeSimulations.value(QGuiApplication::primaryScreen()));
        QVERIFY(world);
        settings.setAnimationSpeed(125);
        QCOMPARE(world->config().speed, 125);
        manager.hide();
        QCoreApplication::sendPostedEvents(nullptr, QEvent::DeferredDelete);
        QVERIFY(!world);
        QCOMPARE(manager.m_pendingViewDeletions, 0);
    }
};
QTEST_MAIN(OverlaySnakesTest)
#include "test_overlaysnakes.moc"

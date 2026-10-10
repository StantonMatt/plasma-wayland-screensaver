// SPDX-License-Identifier: GPL-3.0-or-later
#include "overlaymanager.h"
#include "snakesimulation.h"
#include "snakerenderer.h"
#include "configuration.h"
#include "presentationclock.h"
#include <QGuiApplication>
#include <QMouseEvent>
#include <QKeyEvent>
#include <QQuickView>
#include <QQmlExpression>
#include <QQmlContext>
#include <QScreen>
#include <QSignalSpy>
#include <QTemporaryDir>
#include <QTest>

class OverlaySnakesTest final : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void seasonalMidnightSwitchAndOverride()
    {
        const QByteArray previous = qgetenv("PVS_SEASON");
        const bool wasSet = qEnvironmentVariableIsSet("PVS_SEASON");
        qputenv("PVS_SEASON", "auto");
        QTemporaryDir dir;
        Configuration settings(dir.filePath(QStringLiteral("settingsrc")));
        settings.setVisualModule(QStringLiteral("none"));
        OverlayManager manager(&settings);
        QVERIFY(manager.show());
        QVERIFY(manager.m_seasonTimer.isSingleShot());
        QCOMPARE(manager.m_seasonTimer.timerType(), Qt::PreciseTimer);
        QVERIFY(manager.m_seasonTimer.isActive());
        auto *root = manager.m_views.constBegin().value()->rootObject();
        QVERIFY(root);
        // Drive the same cached-date update used by the midnight callback.
        manager.m_seasonDate = QDate(2026,10,23); manager.updateSeason();
        QCOMPARE(root->property("season").toInt(), 0);
        manager.m_seasonDate = QDate(2026,10,24); manager.updateSeason();
        QCOMPARE(root->property("season").toInt(), 1);
        settings.setSeasonalThemes(false);
        QCOMPARE(root->property("season").toInt(), 0);
        settings.setSeasonalThemes(true);
        QCOMPARE(root->property("season").toInt(), 1);
        manager.m_seasonDate = QDate(2026,11,2); manager.updateSeason();
        QCOMPARE(root->property("season").toInt(), 0);
        qputenv("PVS_SEASON", "halloween");
        manager.updateSeason();
        QCOMPARE(root->property("season").toInt(), 0); // override is sampled once
        manager.m_seasonTimer.setInterval(1); manager.m_seasonTimer.start();
        QTRY_VERIFY(manager.m_seasonTimer.interval() > 1000); // timeout re-arms at next midnight
        QCOMPARE(manager.m_seasonDate, QDate::currentDate());
        manager.hide();
        QVERIFY(!manager.m_seasonTimer.isActive());
        OverlayManager forced(&settings);
        settings.setSeasonalThemes(false);
        QVERIFY(forced.show());
        QCOMPARE(forced.m_views.constBegin().value()->rootObject()->property("season").toInt(), 1);
        forced.hide();
        if (wasSet) qputenv("PVS_SEASON", previous); else qunsetenv("PVS_SEASON");
    }

    void mappingMotionHasGraceButDeliberateInputDismisses()
    {
        QTemporaryDir dir;
        Configuration settings(dir.filePath(QStringLiteral("settingsrc")));
        settings.setVisualModule(QStringLiteral("none"));
        OverlayManager manager(&settings);
        QVERIFY(manager.show());
        QSignalSpy input(&manager, &OverlayManager::inputDetected);
        auto *view = manager.m_views.value(QGuiApplication::primaryScreen());
        QVERIFY(view);
        QMouseEvent motion(QEvent::MouseMove, QPointF(20, 20), QPointF(20, 20),
                           Qt::NoButton, Qt::NoButton, Qt::NoModifier);
        QVERIFY(manager.inputGraceActive());
        QVERIFY(!manager.eventFilter(view, &motion));
        QCOMPARE(input.count(), 0);
        QKeyEvent key(QEvent::KeyPress, Qt::Key_Escape, Qt::NoModifier);
        QVERIFY(manager.eventFilter(view, &key));
        QCOMPARE(input.count(), 1);
        QTRY_VERIFY(view->isExposed());
        // Exercise the final output's asynchronous mapping completion.
        manager.m_mappingViews.insert(view);
        QEvent expose(QEvent::Expose);
        manager.eventFilter(view, &expose);
        QVERIFY(manager.m_mappingViews.isEmpty());
        QVERIFY(manager.inputGraceActive());
        manager.eventFilter(view, &motion);
        QCOMPARE(input.count(), 1);
        QTRY_VERIFY(!manager.inputGraceActive());
        QVERIFY(manager.eventFilter(view, &motion));
        QCOMPARE(input.count(), 2);
        QSignalSpy teardown(&manager, &OverlayManager::teardownCompleted);
        manager.hide();
        QVERIFY(manager.m_pendingViewDeletions > 0);
        QVERIFY(!manager.show());
        QCOMPARE(teardown.count(), 0);
        QCoreApplication::sendPostedEvents(nullptr, QEvent::DeferredDelete);
        QCOMPARE(manager.m_pendingViewDeletions, 0);
        QCOMPARE(teardown.count(), 0);
        QTRY_COMPARE(teardown.count(), 1);
        QVERIFY(manager.show());
        manager.hide();
        QTRY_COMPARE(teardown.count(), 2);
    }

    void teardownWaitsForEveryRetiredView()
    {
        QTemporaryDir dir;
        Configuration settings(dir.filePath(QStringLiteral("settingsrc")));
        settings.setVisualModule(QStringLiteral("none"));
        OverlayManager manager(&settings);
        QVERIFY(manager.show());
        auto *view = manager.m_views.value(QGuiApplication::primaryScreen());
        QVERIFY(view);
        // Include a view already retired by output removal, in addition to the
        // output still visible when dismissal starts. All are native offscreen.
        auto *removedView = new QQuickView;
        removedView->create();
        manager.retireView(removedView);
        QSignalSpy teardown(&manager, &OverlayManager::teardownCompleted);
        manager.hide();
        QCOMPARE(manager.m_pendingViewDeletions, 2);
        QCoreApplication::sendPostedEvents(removedView, QEvent::DeferredDelete);
        QCOMPARE(manager.m_pendingViewDeletions, 1);
        QCoreApplication::processEvents();
        QCOMPARE(teardown.count(), 0);
        QVERIFY(!manager.show());
        QCoreApplication::sendPostedEvents(view, QEvent::DeferredDelete);
        QCOMPARE(manager.m_pendingViewDeletions, 0);
        QTRY_COMPARE(teardown.count(), 1);
    }

    void reducedMotionPausesSnakes_data()
    {
        QTest::addColumn<QString>("behavior");
        QTest::newRow("independent") << QStringLiteral("independent");
        QTest::newRow("synchronized") << QStringLiteral("synchronized");
        QTest::newRow("seamless") << QStringLiteral("seamless");
    }
    void reducedMotionPausesSnakes()
    {
        QFETCH(QString, behavior);
        QTemporaryDir dir;
        Configuration settings(dir.filePath(QStringLiteral("settingsrc")));
        settings.setVisualModule(QStringLiteral("snakes"));
        settings.setMonitorBehavior(behavior);
        settings.setReducedMotion(true);
        settings.setShowClock(true);
        settings.setClockMovement(QStringLiteral("bounce"));
        OverlayManager manager(&settings);
        QVERIFY(manager.show());
        auto *screen = QGuiApplication::primaryScreen();
        auto *root = manager.m_views.value(screen)->rootObject();
        auto *visual = root->findChild<QQuickItem *>(QStringLiteral("snakeVisualRoot"));
        auto *scrim = root->findChild<QQuickItem *>(QStringLiteral("clockScrim"));
        QVERIFY(visual); QVERIFY(scrim);
        QCOMPARE(scrim->property("status").toInt(), 1); // Image.Ready: qrc texture ships.
        QCOMPARE(visual->property("reducedMotion").toBool(), true);
        auto *renderer = visual->findChild<SnakeRenderer *>(QStringLiteral("snakeNativeRenderer"));
        QVERIFY(renderer);
        QVERIFY(renderer->shaderTimeFrozen());
        QVERIFY(!manager.m_sharedAnimationActive); // Bouncing clock remains still.
        auto *world = manager.m_snakeSimulations.value(screen);
        QVERIFY(world);
        if (behavior != QStringLiteral("independent"))
            QCOMPARE(world, manager.m_sharedSnakeSimulation.get());
        QSignalSpy presentationTicks(manager.m_presentationClocks.value(screen),
                                     &PresentationClock::presentationTick);
        const auto firstTick = world->frame().info.tick;
        const auto firstTime = world->simulationTime();
        QTest::qWait(120);
        QCOMPARE(presentationTicks.count(), 0);
        QCOMPARE(world->frame().info.tick, firstTick);
        QCOMPARE(world->simulationTime(), firstTime);
        world->advance(1.0);
        manager.advanceSnakeSimulation(screen, 1'000'000'000);
        manager.advanceSnakeSimulation(screen, 2'000'000'000);
        QCOMPARE(world->frame().info.tick, firstTick);
        settings.setReducedMotion(false);
        QCOMPARE(root->property("reducedMotion").toBool(), false);
        QVERIFY(!renderer->shaderTimeFrozen());
        // Toggling the setting does not advance time; the real clock resumes.
        QCOMPARE(world->frame().info.tick, firstTick);
        QTRY_VERIFY(world->frame().info.tick > firstTick);
        QVERIFY(presentationTicks.count() > 0);
        settings.setReducedMotion(true);
        QCOMPARE(visual->property("reducedMotion").toBool(), true);
        QVERIFY(renderer->shaderTimeFrozen());
        const auto nextTick = world->frame().info.tick;
        const auto nextTime = world->simulationTime();
        presentationTicks.clear();
        QTest::qWait(120);
        QCOMPARE(presentationTicks.count(), 0);
        QCOMPARE(world->frame().info.tick, nextTick);
        QCOMPARE(world->simulationTime(), nextTime);
        settings.setReducedMotion(false);
        QTRY_VERIFY(world->frame().info.tick > nextTick);
        manager.hide();
        QCoreApplication::sendPostedEvents(nullptr, QEvent::DeferredDelete);
        QCOMPARE(manager.m_pendingViewDeletions, 0);
    }

    void viewportCoordinatesFollowWorld_data()
    {
        QTest::addColumn<QString>("visual");
        QTest::addColumn<QString>("behavior");
        for (const QString &visual : {QStringLiteral("bounce"), QStringLiteral("snakes"), QStringLiteral("none")}) {
            for (const QString &mode : {QStringLiteral("independent"), QStringLiteral("synchronized"), QStringLiteral("seamless")}) {
                QTest::newRow(qPrintable(visual + QLatin1Char('-') + mode)) << visual << mode;
            }
        }
    }
    void viewportCoordinatesFollowWorld()
    {
        QFETCH(QString, visual); QFETCH(QString, behavior);
        QTemporaryDir dir;
        Configuration settings(dir.filePath(QStringLiteral("settingsrc")));
        settings.setVisualModule(visual);
        settings.setMonitorBehavior(behavior);
        settings.setCoverPanels(false);
        settings.setReducedMotion(true);
        settings.setShowClock(true);
        settings.setClockMovement(QStringLiteral("bounce"));
        OverlayManager manager(&settings);
        QVERIFY(manager.show());
        auto *screen = QGuiApplication::primaryScreen();
        auto *view = manager.m_views.value(screen);
        auto *root = view->rootObject();
        QVERIFY(root);
        const QRect viewport(-100, 60, 620, 420);
        view->setGeometry(viewport);
        // A window configure must refresh coordinate properties without a
        // QScreen change or an explicit updateAllViewGeometry call.
        QTRY_COMPARE(root->property("screenX").toReal(), qreal(viewport.x()));
        QCOMPARE(root->property("screenY").toReal(), qreal(viewport.y()));
        QCOMPARE(root->property("virtualX").toReal(), qreal(viewport.x()));
        QCOMPARE(root->property("virtualY").toReal(), qreal(viewport.y()));
        QCOMPARE(root->property("virtualWidth").toReal(), qreal(viewport.width()));
        QCOMPARE(root->property("virtualHeight").toReal(), qreal(viewport.height()));
        QCOMPARE(root->property("sharedClockX").toReal(), manager.m_animationState.clockX());
        QCOMPARE(root->property("sharedClockY").toReal(), manager.m_animationState.clockY());
        auto *second = new QQuickView;
        second->setGeometry(700, -40, 500, 300);
        manager.m_views.insert(nullptr, second);
        manager.updateAnimationState();
        const QRect arena = viewport.united(second->geometry());
        QCOMPARE(root->property("virtualX").toReal(), qreal(arena.x()));
        QCOMPARE(root->property("virtualY").toReal(), qreal(arena.y()));
        QCOMPARE(root->property("virtualWidth").toReal(), qreal(arena.width()));
        QCOMPARE(root->property("virtualHeight").toReal(), qreal(arena.height()));
        QCOMPARE(root->property("sharedClockX").toReal(), manager.m_animationState.clockX());
        QCOMPARE(root->property("sharedClockY").toReal(), manager.m_animationState.clockY());
        // Exercise the actual QML clock transform after a panel offset.
        // No clock ticks are sent, so these bindings must respond themselves.
        root->setProperty("reducedMotion", false);
        root->setProperty("sharedClockX", 20);
        root->setProperty("sharedClockY", 90);
        QQmlExpression x(qmlContext(root), root, QStringLiteral("clockLocalX(100)"));
        QQmlExpression y(qmlContext(root), root, QStringLiteral("clockLocalY(60)"));
        const auto localX = x.evaluate(), localY = y.evaluate();
        QVERIFY(!x.hasError()); QVERIFY(!y.hasError());
        if (behavior == QStringLiteral("seamless")) {
            QCOMPARE(localX.toReal(), qreal(120));
            QCOMPARE(localY.toReal(), qreal(30));
            QCOMPARE(manager.m_animationState.bounds(), QRectF(arena));
        } else {
            QVERIFY(localX.toReal() >= 0 && localX.toReal() <= viewport.width() - 100);
            QVERIFY(localY.toReal() >= 0 && localY.toReal() <= viewport.height() - 60);
        }
        manager.hide();
        QCoreApplication::sendPostedEvents(nullptr, QEvent::DeferredDelete);
    }

    void sharedBallsSurviveRefreshAndHotplug_data()
    {
        QTest::addColumn<QString>("behavior");
        QTest::newRow("synchronized") << QStringLiteral("synchronized");
        QTest::newRow("seamless") << QStringLiteral("seamless");
    }
    void sharedBallsSurviveRefreshAndHotplug()
    {
        QFETCH(QString, behavior);
        QTemporaryDir dir;
        Configuration settings(dir.filePath(QStringLiteral("settingsrc")));
        settings.setVisualModule(QStringLiteral("bounce"));
        settings.setMonitorBehavior(behavior);
        settings.setShowClock(false);
        OverlayManager manager(&settings);
        QVERIFY(manager.show());
        auto *screen = QGuiApplication::primaryScreen();
        auto *first = manager.m_views.value(screen);
        QVERIFY(manager.m_sharedAnimationActive);
        QElapsedTimer timer; timer.start();
        const qint64 origin = timer.msecsSinceReference() * 1'000'000;
        manager.m_animationState.advanceTo(origin);
        manager.m_animationState.advanceTo(origin + 100'000'000);
        const auto balls = manager.m_animationState.balls();
        const auto sample = manager.m_animationState.ballsAt(origin + 50'000'000);
        manager.updateAnimationState(); // Same configuration / new display cadence.
        QCOMPARE(manager.m_animationState.balls(), balls);
        QCOMPARE(manager.m_animationState.ballsAt(origin + 50'000'000), sample);
        auto *second = new QQuickView;
        second->setGeometry(first->width(), 0, first->width(), first->height());
        manager.m_views.insert(nullptr, second);
        manager.updateAnimationState();
        QCOMPARE(manager.m_animationState.balls(), balls);
        const QSizeF arena = manager.m_animationState.bounds().size();
        QCOMPARE(arena, behavior == QStringLiteral("seamless")
                       ? QSizeF(first->width() * 2, first->height()) : QSizeF(first->size()));
        manager.removeScreen(screen);
        if (behavior == QStringLiteral("synchronized")) QCOMPARE(manager.m_animationState.balls(), balls);
        else {
            // Seamless bodies on the removed output relocate to valid pixels.
            const auto survivors = manager.m_animationState.balls();
            QCOMPARE(survivors.size(), balls.size());
            for (int i = 0; i < survivors.size(); ++i) {
                const auto body = survivors[i].toMap(), before = balls[i].toMap();
                QCOMPARE(body.value(QStringLiteral("vx")), before.value(QStringLiteral("vx")));
                QCOMPARE(body.value(QStringLiteral("vy")), before.value(QStringLiteral("vy")));
                QVERIFY(manager.m_animationState.containsRect(QRectF(body.value(QStringLiteral("x")).toReal(),
                    body.value(QStringLiteral("y")).toReal(), body.value(QStringLiteral("size")).toReal(),
                    body.value(QStringLiteral("size")).toReal())));
            }
        }
        QVERIFY(manager.m_sharedAnimationActive);
        QCOMPARE(manager.m_animationState.bounds().size(), QSizeF(second->size()));
        manager.hide();
        QCoreApplication::sendPostedEvents(nullptr, QEvent::DeferredDelete);
        QCOMPARE(manager.m_pendingViewDeletions, 0);
    }

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
            const auto pausedTick = world->frame().info.tick;
            world->advance(1.0);
            manager.advanceSnakeSimulation(screen, 1'000'000'000);
            manager.advanceSnakeSimulation(screen, 2'000'000'000);
            QCOMPARE(world->frame().info.tick, pausedTick);
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
        manager.advanceSnakeSimulation(screen, 1'000'000'000);
        manager.advanceSnakeSimulation(nullptr, 2'000'000'000);
        QCOMPARE(world->frame().info.tick, 0U); // Both viewports share the pause.
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
        manager.advanceSnakeSimulation(screen, 1'000'000'000);
        manager.advanceSnakeSimulation(screen, 1'033'333'334);
        QCOMPARE(world->frame().info.tick, 1U);
        // A second viewport shares the timeline; overlapping requests do not step twice.
        manager.m_snakeSimulations.insert(nullptr, world);
        manager.advanceSnakeSimulation(nullptr, 1'020'000'000);
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

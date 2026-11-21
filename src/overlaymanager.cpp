// SPDX-License-Identifier: GPL-3.0-or-later
#include "overlaymanager.h"

#include "configuration.h"
#include "fireflyrenderer.h"
#include "livingvisualitem.h"
#include "matrixrenderer.h"
#include "presentationclock.h"
#include "snakerenderer.h"
#include "snakesimulation.h"
#include "strokerenderer.h"
#include "visualdepthregistry.h"
#include "visualrenderer.h"
#include "visualworldcoordinator.h"
#include <QColor>
#include <QDataStream>
#include <cmath>

#include <LayerShellQt/Window>
#include <QCoreApplication>
#include <QDateTime>
#include <QEvent>
#include <QGuiApplication>
#include <QPointer>
#include <QQuickItem>
#include <QQuickView>
#include <QRandomGenerator>
#include <QScreen>
#include <QSet>
#include <QString>
#include <QTimer>
#include <QUrl>
#include <QVariant>

#include <algorithm>
#include <limits>
#include <utility>

#if defined(__GLIBC__)
#include <malloc.h>
#endif

OverlayManager::OverlayManager(Configuration* configuration, QObject* parent)
    : QObject(parent)
    , m_configuration(configuration)
    , m_depthCoordinator(std::make_unique<VisualWorldCoordinator>())
    , m_animationState(this)
{
    registerSnakeTypes();
    connect(m_configuration, &Configuration::changed, this, [this] {
        if (!m_visible) return;
        m_depthLayoutKey.clear();
        configureDepthWorlds();
        configureSnakeRenderSharing();
        if (m_sharedSnakeSimulation) m_sharedSnakeSimulation->applySettings(*m_configuration);
        else for (auto *simulation : std::as_const(m_snakeSimulations)) simulation->applySettings(*m_configuration);
        updateAllViewGeometry();
    });
    connect(qGuiApp, &QGuiApplication::screenAdded, this, [this](QScreen *screen) {
        const QPointer<QScreen> guardedScreen(screen);
        QMetaObject::invokeMethod(this, [this, guardedScreen] {
            if (m_visible && guardedScreen) {
                addScreen(guardedScreen.data());
            }
        }, Qt::QueuedConnection);
    }, Qt::DirectConnection);
    connect(qGuiApp, &QGuiApplication::screenRemoved, this, &OverlayManager::removeScreen,
            Qt::DirectConnection);
}

OverlayManager::~OverlayManager()
{
    hide();
}

bool OverlayManager::show()
{
    if (m_visible) {
        return true;
    }

    if (m_teardownPending || m_pendingViewDeletions != 0) return false;

    m_visible = true;
    m_depthGpuFailed = false;
    m_inputGraceTimer.start();
    qApp->installEventFilter(this);
    m_animationEpochMs = QDateTime::currentMSecsSinceEpoch();
    m_depthEpochNs = VisualWorldCoordinator::monotonicNow();
    m_depthSeed = QRandomGenerator::global()->generate64() | 1U;
    m_depthLayoutKey.clear();
    updateAnimationState();
    const QList<QScreen *> screens = QGuiApplication::screens();
    for (QScreen *screen : screens) {
        addScreen(screen);
    }
    if (m_views.isEmpty()) {
        hide();
        return false;
    }
    return true;
}

void OverlayManager::hide()
{
    if (m_teardownPending) return;
    m_teardownPending = true;
    m_visible = false;
    m_mappingViews.clear();
    m_depthCoordinator->stop();
    m_depthItems.clear();
    m_depthLayoutKey.clear();
    m_animationState.stop();
    m_ballArenaScreen = nullptr;
    m_sharedAnimationActive = false;
    m_animationDriverScreen = nullptr;
    m_snakeSimulations.clear();
    m_sharedSnakeSimulation.reset();
    m_snakeArenaScreen = nullptr;
    m_snakeBehavior.clear();
    m_presentationClocks.clear();
    m_snakeRenderers.clear();
    qApp->removeEventFilter(this);
    const auto views = m_views;
    m_views.clear();
    m_screenGeometries.clear();
    for (QQuickView *view : views) {
        retireView(view);
    }
    if (m_pendingViewDeletions == 0) {
        QTimer::singleShot(0, this, &OverlayManager::finishTeardown);
    }
}

bool OverlayManager::isVisible() const
{
    return m_visible;
}

bool OverlayManager::inputGraceActive() const
{
    constexpr int mappingGraceMilliseconds = 500;
    return m_visible && (!m_mappingViews.isEmpty()
        || (m_inputGraceTimer.isValid() && m_inputGraceTimer.elapsed() < mappingGraceMilliseconds));
}

void OverlayManager::finishTeardown()
{
    if (!m_teardownPending || m_visible || m_pendingViewDeletions != 0) return;
    m_teardownPending = false;
    reclaimReleasedMemory();
    Q_EMIT teardownCompleted();
}

void OverlayManager::setDeveloperMode(bool enabled)
{
    m_developerMode = enabled;
}

bool OverlayManager::addScreen(QScreen *screen)
{
    if (!screen || m_views.contains(screen)) {
        return screen != nullptr;
    }

    auto *view = new QQuickView;
    // Overlay windows are short lived. Their scene graphs, swapchains and
    // Canvas backing stores must be releasable as soon as the saver dismisses.
    // QQuickWindow keeps both categories persistent by default.
    view->setPersistentGraphics(false);
    view->setPersistentSceneGraph(false);
    view->setObjectName(QStringLiteral("screensaver-%1").arg(screen->name()));
    view->setColor(Qt::black);
    view->setResizeMode(QQuickView::SizeRootObjectToView);
    view->setFlags(Qt::FramelessWindowHint | Qt::WindowStaysOnTopHint);
    view->setScreen(screen);
    view->setGeometry(screen->geometry());

    int presentationRate = m_configuration->frameRate();
    // Snakes use 30 Hz physics with interpolated presentation at up to 60 Hz.
    // Presenting the same interpolation more often only burns CPU/GPU bandwidth
    // without adding motion detail. Other visual modules retain the user's rate.
    if (m_configuration->visualModule() == QStringLiteral("snakes")
            && (presentationRate == 0 || presentationRate > 60)) {
        presentationRate = 60;
    }
    auto *presentationClock = new PresentationClock(view, presentationRate, view);
    connect(presentationClock, &PresentationClock::presentationTick, this,
            [this, screen](qint64 presentationNanoseconds) {
                if (m_sharedAnimationActive) m_animationState.advanceTo(presentationNanoseconds);
                advanceSnakeSimulation(screen, presentationNanoseconds);
                if (depthActive())
                {
#ifdef PVS_ENABLE_VISUAL_CAPTURE
                    if (m_captureManualTime)
                        return;
#endif
                    m_depthCoordinator->present(screen->name(), presentationNanoseconds);
                }
            });

    const uint seed = m_configuration->monitorBehavior() == QStringLiteral("synchronized")
        || m_configuration->monitorBehavior() == QStringLiteral("seamless")
        ? 1U : qHash(screen->name());
    const bool sharedMotion = m_configuration->monitorBehavior() != QStringLiteral("independent");
    const QRect screenGeometry = screen->geometry();
    const QRect virtualGeometry = screen->virtualGeometry();
    view->setInitialProperties({
        {QStringLiteral("depthActive"), depthActive()},
        { QStringLiteral("visualModule"), m_configuration->visualModule() },
        { QStringLiteral("backgroundStyle"), m_configuration->backgroundStyle() },
        { QStringLiteral("animationSpeed"), m_configuration->animationSpeed() },
        { QStringLiteral("animationDensity"), m_configuration->animationDensity() },
        { QStringLiteral("animationScale"), m_configuration->animationScale() },
        { QStringLiteral("animationPalette"), m_configuration->animationPalette() },
        { QStringLiteral("trailAmount"), m_configuration->trailAmount() },
        { QStringLiteral("ballCount"), std::min(20, m_configuration->ballCount()) },
        { QStringLiteral("ballGravity"), m_configuration->ballGravity() },
        { QStringLiteral("ballElasticity"), m_configuration->ballElasticity() },
        { QStringLiteral("ballCollisions"), m_configuration->ballCollisions() },
        { QStringLiteral("snakeIntelligence"), m_configuration->snakeIntelligence() },
        { QStringLiteral("snakeSelfCollisions"), m_configuration->snakeSelfCollisions() },
        { QStringLiteral("snakeDeadlyWalls"), m_configuration->snakeDeadlyWalls() },
        { QStringLiteral("developerMode"), m_developerMode },
        { QStringLiteral("showClock"), m_configuration->showClock() },
        { QStringLiteral("clockMovement"), m_configuration->clockMovement() },
        { QStringLiteral("clockSpeed"), m_configuration->clockSpeed() },
        { QStringLiteral("frameRate"), m_configuration->frameRate() },
        { QStringLiteral("reducedMotion"), m_configuration->reducedMotion() },
        { QStringLiteral("monitorBehavior"), m_configuration->monitorBehavior() },
        { QStringLiteral("seed"), seed },
        { QStringLiteral("animationEpochMs"), sharedMotion ? m_animationEpochMs : QDateTime::currentMSecsSinceEpoch() },
        { QStringLiteral("screenX"), screenGeometry.x() },
        { QStringLiteral("screenY"), screenGeometry.y() },
        { QStringLiteral("virtualX"), virtualGeometry.x() },
        { QStringLiteral("virtualY"), virtualGeometry.y() },
        { QStringLiteral("virtualWidth"), virtualGeometry.width() },
        { QStringLiteral("virtualHeight"), virtualGeometry.height() },
        { QStringLiteral("animationState"), QVariant::fromValue(static_cast<QObject*>(&m_animationState)) },
        { QStringLiteral("presentationClock"), QVariant::fromValue(static_cast<QObject*>(presentationClock)) },
    });
    view->setSource(QUrl(QStringLiteral("qrc:/qml/Screensaver.qml")));
    if (view->status() == QQuickView::Error) {
        delete view;
        return false;
    }

    if (m_configuration->visualModule() == QStringLiteral("snakes")) {
        if (auto *rootItem = qobject_cast<QQuickItem *>(view->rootObject())) {
            if (auto *snakeRoot = rootItem->findChild<QQuickItem *>(
                    QStringLiteral("snakeVisualRoot"), Qt::FindChildrenRecursively)) {
                if (auto *renderer = snakeRoot->findChild<SnakeRenderer *>(QStringLiteral("snakeNativeRenderer"))) {
                    m_snakeRenderers.insert(screen, renderer);
                }
            }
        }
    }

    if (!depthActive())
        attachLegacyRenderer(view, presentationClock);

    if (depthActive())
    {
        if (auto* root = qobject_cast<QQuickItem*>(view->rootObject()))
        {
            root->setProperty("depthActive", true);
            auto* layer = root->findChild<QQuickItem*>(QStringLiteral("livingVisualLayer"));
            Q_ASSERT(layer);
            auto* item
                = VisualDepthRegistry::createItem(VisualDepthRegistry::id(m_configuration->visualModule()), layer);
            item->setObjectName(QStringLiteral("livingVisual"));
            item->setSize(root->size());
            // Above the background/legacy visual, below the clock.
            connect(root, &QQuickItem::widthChanged, item, [root, item] { item->setWidth(root->width()); });
            connect(root, &QQuickItem::heightChanged, item, [root, item] { item->setHeight(root->height()); });
            m_depthItems.insert(screen, item);
            connect(item, &LivingVisualItem::gpuUnavailable, this, [this, item] {
                if (!m_visible || m_depthGpuFailed || !m_depthItems.values().contains(item)) return;
                m_depthGpuFailed = true;
                qWarning("Living visual GPU probe failed; restoring the legacy renderer");
                m_depthCoordinator->stop();
                for (auto it = m_views.cbegin(); it != m_views.cend(); ++it) {
                    it.value()->rootObject()->setProperty("depthActive", false);
                    attachLegacyRenderer(it.value(), m_presentationClocks.value(it.key()));
                }
                updateAnimationState();
            }, Qt::QueuedConnection);
        }
    }

    auto *layer = LayerShellQt::Window::get(view);
    layer->setScreen(screen);
    layer->setWantsToBeOnActiveScreen(false);
    layer->setLayer(LayerShellQt::Window::LayerOverlay);
    LayerShellQt::Window::Anchors anchors;
    anchors.setFlag(LayerShellQt::Window::AnchorTop);
    anchors.setFlag(LayerShellQt::Window::AnchorBottom);
    anchors.setFlag(LayerShellQt::Window::AnchorLeft);
    anchors.setFlag(LayerShellQt::Window::AnchorRight);
    layer->setAnchors(anchors);
    // -1 tells layer-shell not to shrink around panel exclusive zones, so the
    // overlay covers taskbars without changing their Plasma configuration.
    layer->setExclusiveZone(m_configuration->coverPanels() ? -1 : 0);
    layer->setKeyboardInteractivity(LayerShellQt::Window::KeyboardInteractivityExclusive);
    layer->setScope(QStringLiteral("plasma-visual-screensaver"));

    connect(screen, &QScreen::geometryChanged, view, [this](const QRect &) {
        updateAllViewGeometry();
    });
    connect(screen, &QScreen::virtualGeometryChanged, view, [this](const QRect &) {
        updateAllViewGeometry();
    });
    connect(screen, &QScreen::refreshRateChanged, view, [this](qreal) {
        updateAnimationState();
    });
    m_views.insert(screen, view);
    m_screenGeometries.insert(screen, screenGeometry);
    m_presentationClocks.insert(screen, presentationClock);
    // Layer-shell configure events can shrink/move the overlay around panels
    // without changing QScreen geometry. Recompute every snake mode from the
    // resulting windows, without sending another full-screen size request.
    const auto viewportChanged = [this] { updateAnimationState(); };
    connect(view, &QWindow::widthChanged, this, viewportChanged);
    connect(view, &QWindow::heightChanged, this, viewportChanged);
    connect(view, &QWindow::xChanged, this, viewportChanged);
    connect(view, &QWindow::yChanged, this, viewportChanged);
    m_mappingViews.insert(view);
    view->show();
    updateAllViewGeometry();
    return true;
}

void OverlayManager::removeScreen(QScreen *screen)
{
    m_snakeSimulations.remove(screen);
    m_depthItems.remove(screen);
    m_depthLayoutKey.clear();
    if (m_snakeArenaScreen == screen) m_snakeArenaScreen = nullptr;
    if (m_ballArenaScreen == screen) m_ballArenaScreen = nullptr;
    m_presentationClocks.remove(screen);
    m_snakeRenderers.remove(screen);
    if (m_animationDriverScreen == screen) {
        m_animationDriverScreen = nullptr;
    }
    QQuickView *view = m_views.take(screen);
    m_screenGeometries.remove(screen);
    if (view) {
        m_mappingViews.remove(view);
        retireView(view);
    }
    if (m_visible && m_views.isEmpty()) {
        Q_EMIT overlayUnavailable();
    } else {
        updateAllViewGeometry();
    }
}

void OverlayManager::retireView(QQuickView *view)
{
    if (!view) {
        return;
    }

    if (auto *clock = view->findChild<PresentationClock *>()) clock->setRunning(false);

    // Stop rendering before the deferred QObject destruction. This asks the
    // render thread to discard per-window caches and releases the native
    // Wayland surface immediately, rather than retaining them until an
    // arbitrary later event-loop turn.
    view->hide();
    view->releaseResources();
    view->destroy();

    ++m_pendingViewDeletions;
    connect(view, &QObject::destroyed, this, [this] {
        // QQuickWindow's destructor joins its render thread before QObject
        // emits destroyed. This counter and the deferred trim stay GUI-owned.
        --m_pendingViewDeletions;
        if (m_pendingViewDeletions == 0 && !m_visible) {
            // Run after QQuickView's destructor has joined its Canvas/render
            // workers. Those workers use separate glibc allocation arenas.
            QTimer::singleShot(0, this, &OverlayManager::finishTeardown);
        }
    });
    view->deleteLater();
}

void OverlayManager::reclaimReleasedMemory()
{
    if (m_visible || m_pendingViewDeletions != 0) {
        return;
    }

#if defined(__GLIBC__)
    // Qt has now destroyed the heavy QML scene. glibc otherwise keeps many of
    // the freed Canvas and scene-graph pages in its process arenas for reuse,
    // which made the idle daemon appear to retain gigabytes for days.
    malloc_trim(0);
#endif
}

void OverlayManager::updateAllViewGeometry()
{
    updateAnimationState();
    const auto screens = m_views.keys();
    for (QScreen *screen : screens) {
        updateViewGeometry(screen);
    }
    configureSnakeRenderSharing();
}

void OverlayManager::updateAnimationState()
{
    configureDepthWorlds();
    const bool seamless = m_configuration->monitorBehavior() == QStringLiteral("seamless");
    const bool motionAllowed = !m_configuration->reducedMotion();
    const bool animateBall = !depthActive() && m_configuration->monitorBehavior() != QStringLiteral("independent") && motionAllowed
        && m_configuration->visualModule() == QStringLiteral("bounce");
    const bool animateClock = seamless && motionAllowed && m_configuration->showClock()
        && m_configuration->clockMovement() == QStringLiteral("bounce");
    int simulationRate = m_configuration->frameRate();
    if (simulationRate == 0) {
        simulationRate = 0;
        for (QScreen *screen : QGuiApplication::screens()) {
            if (screen) {
                simulationRate = std::max(simulationRate, qRound(screen->refreshRate()));
            }
        }
        if (simulationRate <= 0) {
            simulationRate = 60;
        }
    }
    if (!m_ballArenaScreen || !m_views.contains(m_ballArenaScreen)) {
        m_ballArenaScreen = m_views.contains(QGuiApplication::primaryScreen())
            ? QGuiApplication::primaryScreen() : (m_views.isEmpty() ? nullptr : m_views.constBegin().key());
    }
    QList<QRect> geometries;
    QRect virtualGeometry;
    for (auto *view : std::as_const(m_views)) {
        geometries.append(view->geometry());
        virtualGeometry = virtualGeometry.united(view->geometry());
    }
    // World coordinates and every QML world-to-view transform use configured
    // overlay viewports, including panel insets and negative desktop origins.
    // Window configure signals enter here without any QScreen geometry change.
    for (auto *view : std::as_const(m_views)) {
        if (QObject *root = view->rootObject()) {
            root->setProperty("screenX", view->x());
            root->setProperty("screenY", view->y());
            root->setProperty("virtualX", virtualGeometry.x());
            root->setProperty("virtualY", virtualGeometry.y());
            root->setProperty("virtualWidth", virtualGeometry.width());
            root->setProperty("virtualHeight", virtualGeometry.height());
        }
    }
    if (!seamless) {
        geometries.clear();
        const QSize size = m_views.contains(m_ballArenaScreen) ? m_views.value(m_ballArenaScreen)->size()
            : (QGuiApplication::primaryScreen() ? QGuiApplication::primaryScreen()->size() : QSize(1920, 1080));
        geometries.append(QRect(QPoint(0, 0), size));
    }
    m_animationState.configureGeometries(geometries, animateBall, animateClock,
                               m_configuration->clockSpeed(), simulationRate,
                               m_configuration->ballCount(), m_configuration->animationSpeed(),
                               m_configuration->animationScale(), m_configuration->ballGravity(),
                               m_configuration->ballElasticity(), m_configuration->ballCollisions(),
                               animateBall || animateClock);
    m_sharedAnimationActive = animateBall || animateClock;
    updatePresentationClocks();
}

void OverlayManager::updatePresentationClocks()
{
    m_animationDriverScreen = nullptr;
    int presentationRate = m_configuration->frameRate();
    if (m_configuration->visualModule() == QStringLiteral("snakes")
        && (presentationRate == 0 || presentationRate > 60)) presentationRate = 60;
    long double fastestPeriod = std::numeric_limits<long double>::max();
    for (QScreen *screen : m_presentationClocks.keys()) {
        if (!screen) {
            continue;
        }
        if (!m_animationDriverScreen) {
            m_animationDriverScreen = screen;
        }
        const auto period = PresentationPacing::periodNanoseconds(presentationRate, screen->refreshRate());
        if (period < fastestPeriod) {
            fastestPeriod = period;
            m_animationDriverScreen = screen;
        }
    }

    qint64 predictionLead = 0;
    for (QScreen *screen : m_presentationClocks.keys()) {
        predictionLead = std::max(predictionLead, qint64(std::ceil(1e9 /
            PresentationPacing::validRefreshRate(screen ? screen->refreshRate() : 60.0))));
    }
    const bool seamless = m_configuration->monitorBehavior() == QStringLiteral("seamless");
    const bool visualUsesClock = m_configuration->visualModule() != QStringLiteral("none")
        && (depthActive() || !(seamless && m_configuration->visualModule() == QStringLiteral("bounce")));
    const bool clockUsesClock = !seamless && m_configuration->showClock()
        && m_configuration->clockMovement() == QStringLiteral("bounce");
    const bool perWindowMotion = !m_configuration->reducedMotion()
        && (visualUsesClock || clockUsesClock);
    configureSnakeRenderSharing();
    for (auto *simulation : std::as_const(m_snakeSimulations)) simulation->setPresentationLead(predictionLead);
    for (auto it = m_presentationClocks.cbegin(); it != m_presentationClocks.cend(); ++it) {
        it.value()->setTargetFrameRate(presentationRate);
        it.value()->setRunning(perWindowMotion || m_sharedAnimationActive);
        if (QQuickView *view = m_views.value(it.key())) {
            if (QObject *root = view->rootObject()) {
                root->setProperty("reducedMotion", m_configuration->reducedMotion());
                root->setProperty("presentationClock",
                                  QVariant::fromValue(static_cast<QObject *>(it.value())));
            }
        }
    }
}

void OverlayManager::advanceSnakeSimulation(QScreen *screen, qint64 presentationNanoseconds)
{
    if (auto *simulation = m_snakeSimulations.value(screen)) {
        simulation->advanceTo(presentationNanoseconds);
        if (auto *renderer = m_snakeRenderers.value(screen)) renderer->presentAt(presentationNanoseconds);
    }
}

void OverlayManager::configureSnakeRenderSharing()
{
    if (m_configuration->visualModule() != QStringLiteral("snakes")) return;
    const QString behavior = m_configuration->monitorBehavior();
    const bool shared = behavior != QStringLiteral("independent");
    const bool seamless = behavior == QStringLiteral("seamless");
    if (m_snakeBehavior != behavior) {
        for (auto *renderer : std::as_const(m_snakeRenderers)) renderer->setSimulation(nullptr);
        if (!m_sharedSnakeSimulation) {
            const QSet<SnakeSimulation *> worlds(m_snakeSimulations.cbegin(), m_snakeSimulations.cend());
            qDeleteAll(worlds);
        }
        m_snakeSimulations.clear();
        m_sharedSnakeSimulation.reset();
        m_snakeArenaScreen = nullptr;
        m_snakeBehavior = behavior;
    }
    if (shared && !m_snakeArenaScreen) m_snakeArenaScreen = m_animationDriverScreen;
    QRect sharedArena;
    if (seamless) {
        for (auto *view : std::as_const(m_views)) sharedArena = sharedArena.united(view->geometry());
    } else if (shared) {
        if (auto *view = m_views.value(m_snakeArenaScreen)) sharedArena = view->geometry();
    }
    for (auto it = m_snakeRenderers.cbegin(); it != m_snakeRenderers.cend(); ++it) {
        QScreen *screen = it.key();
        auto *view = m_views.value(screen);
        if (!view) continue;
        const QRect arena = shared ? sharedArena : view->geometry();
        if (arena.isEmpty()) continue;
        SnakeSimulation *simulation = shared ? m_sharedSnakeSimulation.get() : m_snakeSimulations.value(screen);
        if (!simulation) {
            const auto config = SnakeSimulation::configuration(*m_configuration, arena.width(), arena.height(),
                                                               shared ? 1U : qHash(screen->name()));
            simulation = new SnakeSimulation(config, shared ? static_cast<QObject *>(this) : m_views.value(screen));
            simulation->applySettings(*m_configuration);
            if (shared) m_sharedSnakeSimulation.reset(simulation);
        }
        simulation->resize(arena.width(), arena.height());
        m_snakeSimulations.insert(screen, simulation);
        if (auto *root = view->rootObject()) {
            root->setProperty("snakeSimulation", QVariant::fromValue(simulation));
            root->setProperty("monitorBehavior", behavior);
        }
        it.value()->setSimulation(simulation);
        it.value()->setScaleToViewport(behavior == QStringLiteral("synchronized"));
        it.value()->setDrawOffset(seamless ? arena.x() - view->x() : 0,
                                 seamless ? arena.y() - view->y() : 0);
        it.value()->setDeveloperMode(m_developerMode);
        if (auto *clock = m_presentationClocks.value(screen)) clock->setTraceSimulationSource(simulation);
    }
}

void OverlayManager::updateViewGeometry(QScreen *screen)
{
    QQuickView *view = m_views.value(screen);
    if (!view || !screen) {
        return;
    }
    const QRect screenGeometry = screen->geometry();
    view->setScreen(screen);
    if (m_screenGeometries.value(screen) != screenGeometry) {
        m_screenGeometries.insert(screen, screenGeometry);
        view->setGeometry(screenGeometry);
    }
    LayerShellQt::Window::get(view)->setExclusiveZone(m_configuration->coverPanels() ? -1 : 0);
}

bool OverlayManager::eventFilter(QObject *watched, QEvent *event)
{
    if (m_visible && event->type() == QEvent::Expose) {
        auto *view = qobject_cast<QQuickView *>(watched);
        if (view && view->isExposed() && m_mappingViews.remove(view)) {
            // Wayland mapping finishes asynchronously, after show() returns.
            m_inputGraceTimer.start();
            configureDepthWorlds();
        }
    }
    if (m_visible && isDismissEvent(event)) {
        // Mapping can synthesize pointer/tablet motion. Deliberate key/button,
        // wheel and touch input remains responsive even during the grace period.
        if (inputGraceActive() && (event->type() == QEvent::MouseMove
                                  || event->type() == QEvent::TabletMove)) return false;
        Q_EMIT inputDetected();
        return true;
    }
    return false;
}

bool OverlayManager::isDismissEvent(const QEvent *event) const
{
    switch (event->type()) {
    case QEvent::KeyPress:
    case QEvent::MouseButtonPress:
    case QEvent::MouseButtonRelease:
    case QEvent::MouseMove:
    case QEvent::Wheel:
    case QEvent::TouchBegin:
    case QEvent::TouchUpdate:
    case QEvent::TabletPress:
    case QEvent::TabletMove:
        return true;
    default:
        return false;
    }
}

bool OverlayManager::depthActive() const
{
#ifdef PVS_ENABLE_VISUAL_CAPTURE
    if (m_captureFixture)
        return true;
#endif
    return !m_depthGpuFailed && m_configuration->depthAvailable();
}
void OverlayManager::configureDepthWorlds()
{
    if (!m_visible || !depthActive() || !m_mappingViews.isEmpty()
        || m_views.size() != QGuiApplication::screens().size())
        return;
    QVector<VisualOutput> outputs;
    VisualEnvironment environment;
    environment.presentationLeadNs = 0;
    QByteArray key;
    QDataStream stream(&key, QIODevice::WriteOnly);
    QList<QScreen*> screens = m_views.keys();
    std::sort(screens.begin(), screens.end(), [](auto* a, auto* b) { return a->name() < b->name(); });
    for (auto* screen : screens)
    {
        auto* view = m_views.value(screen);
        outputs.append({ screen->name(), view->geometry(), view->devicePixelRatio(), screen->refreshRate(),
            screen == QGuiApplication::primaryScreen(), view->isExposed() });
        environment.presentationLeadNs = std::max(environment.presentationLeadNs,
            qint64(
                std::ceil(PresentationPacing::periodNanoseconds(m_configuration->frameRate(), screen->refreshRate()))));
        stream << screen->name() << view->geometry() << view->devicePixelRatio();
        if (auto* root = view->rootObject())
            environment.clockRects.insert(screen->name(), root->property("clockRect").toRectF());
    }
    if (key == m_depthLayoutKey)
        return;
    auto visual = VisualDepthRegistry::id(m_configuration->visualModule());
#ifdef PVS_ENABLE_VISUAL_CAPTURE
    if (m_captureFixture)
        visual = 65535;
#endif
    const QStringList palettes = { QStringLiteral("ocean"), QStringLiteral("spectrum"), QStringLiteral("ember"),
        QStringLiteral("forest"), QStringLiteral("mono"), QStringLiteral("pastel") };
    vd_config config { visual, uint32_t(std::max(qsizetype(0), palettes.indexOf(m_configuration->animationPalette()))),
        m_depthSeed, m_configuration->animationSpeed() / 100., double(m_configuration->animationDensity()),
        m_configuration->animationScale() / 100., m_configuration->trailAmount() / 100.,
        uint32_t(m_configuration->reducedMotion()), 0 };
#ifdef PVS_ENABLE_VISUAL_CAPTURE
    if (m_captureSeedSet)
        config.seed = m_captureSeed;
#endif
    std::vector<vd_value> values;
    if (visual != 65535)
        for (const auto& entry : m_configuration->visualSchema())
        {
            const auto p = entry.toMap();
            const auto value = m_configuration->visualParameter(p.value(QStringLiteral("key")).toString());
            const auto kind = p.value(QStringLiteral("kind")).toUInt();
            const auto payload = kind == 3
                ? quint64(p.value(QStringLiteral("choices")).toStringList().indexOf(value.toString()))
                : kind == 2 ? quint64(value.toBool())
                            : quint64(value.toLongLong());
            values.push_back({ p.value(QStringLiteral("id")).toUInt(), kind, payload });
        }
    const auto background = m_configuration->backgroundStyle();
    const QStringList backgrounds
        = { QStringLiteral("black"), QStringLiteral("midnight"), QStringLiteral("ocean"), QStringLiteral("plum") };
    environment.colours.background = uint32_t(std::max(qsizetype(0), backgrounds.indexOf(background)));
    environment.colours.frame_cap = uint32_t(m_configuration->frameRate());
    const QColor top = background == QStringLiteral("black") ? QColor(QStringLiteral("#000000"))
        : background == QStringLiteral("ocean")              ? QColor(QStringLiteral("#02111d"))
        : background == QStringLiteral("plum")               ? QColor(QStringLiteral("#18091d"))
                                                             : QColor(QStringLiteral("#071329"));
    const QColor bottom = background == QStringLiteral("black") ? QColor(QStringLiteral("#000000"))
        : background == QStringLiteral("ocean")                 ? QColor(QStringLiteral("#062f3c"))
        : background == QStringLiteral("plum")                  ? QColor(QStringLiteral("#310d2c"))
                                                                : QColor(QStringLiteral("#13091e"));
    for (int i = 0; i < 4; ++i)
    {
        environment.colours.top[i] = float(i == 0 ? top.redF()
                : i == 1                          ? top.greenF()
                : i == 2                          ? top.blueF()
                                                  : top.alphaF());
        environment.colours.bottom[i] = float(i == 0 ? bottom.redF()
                : i == 1                             ? bottom.greenF()
                : i == 2                             ? bottom.blueF()
                                                     : bottom.alphaF());
    }
    const auto behavior = m_configuration->monitorBehavior();
    const auto mode = behavior == QStringLiteral("seamless") ? VisualMonitorMode::Spanning
        : behavior == QStringLiteral("synchronized")         ? VisualMonitorMode::Mirrored
                                                             : VisualMonitorMode::Independent;
    const auto policy = visuals_depth_mirrored_policy(visual) ? VisualMirroredPolicy::PerOutputLocal
                                                              : VisualMirroredPolicy::SharedCanonical;
    if (m_depthCoordinator->configure(config, values, outputs, mode, policy, environment, m_depthEpochNs))
    {
        m_depthLayoutKey = key;
        for (auto* screen : screens)
            m_depthCoordinator->attach(screen->name(), m_depthItems.value(screen));
        // Publish the first chart as soon as all configured views are attached.
        // An exposed layer can draw it before the first presentation tick.
        for (auto* screen : screens)
            m_depthCoordinator->present(screen->name(),m_depthEpochNs);
    }
}

void OverlayManager::attachLegacyRenderer(QQuickView* view, PresentationClock* clock)
{
    const auto visual = m_configuration->visualModule();
    if (!VisualDepthRegistry::id(visual))
        return;
    auto* root = view->rootObject()->property("legacyVisual").value<QQuickItem*>();
    // Some released visuals draw directly in QML. Others expose a native renderer slot.
    if (!root || root->metaObject()->indexOfProperty("nativeRenderer") < 0
        || root->property("nativeRenderer").value<QObject*>())
        return;

    QQuickItem* item = nullptr;
    if (visual == QStringLiteral("fireflies")) {
        item = new FireflyRenderer(root);
    } else if (visual == QStringLiteral("matrix")) {
        auto* renderer = new MatrixRenderer(root);
        connect(clock, &PresentationClock::presentationTick, renderer, &MatrixRenderer::presentAt);
        item = renderer;
    } else if (visual == QStringLiteral("ribbons") || visual == QStringLiteral("kaleidoscope")) {
        auto* renderer = new StrokeRenderer(root);
        connect(clock, &PresentationClock::presentationTick, renderer, &StrokeRenderer::presentAt);
        item = renderer;
    } else {
        auto* renderer = new VisualRenderer(root);
        connect(clock, &PresentationClock::presentationTick, renderer, &VisualRenderer::presentAt);
        if (visual == QStringLiteral("bounce")) {
            connect(clock, &PresentationClock::frameTick, renderer, &VisualRenderer::presentBounceDelta);
            const QPointer<AnimationState> state(&m_animationState);
            renderer->setBounceSource([state](qint64 ns, visuals_bounce_ball* out, size_t capacity) {
                if (!state) return size_t(0);
                std::array<AnimationState::BallSnapshot, 20> balls;
                const auto count = state->ballSnapshotAt(ns, balls.data(), std::min(capacity, balls.size()));
                for (size_t i = 0; i < count; ++i)
                    out[i] = {balls[i].x, balls[i].y, balls[i].vx, balls[i].vy, balls[i].size};
                return count;
            });
        }
        item = renderer;
    }
    item->setSize(root->size());
    connect(root, &QQuickItem::widthChanged, item, [root, item] { item->setWidth(root->width()); });
    connect(root, &QQuickItem::heightChanged, item, [root, item] { item->setHeight(root->height()); });
    root->setProperty("nativeRenderer", QVariant::fromValue(static_cast<QObject*>(item)));
}

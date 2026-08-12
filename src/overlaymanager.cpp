// SPDX-License-Identifier: GPL-3.0-or-later
#include "overlaymanager.h"

#include "configuration.h"

#include <LayerShellQt/Window>
#include <QCoreApplication>
#include <QDateTime>
#include <QEvent>
#include <QGuiApplication>
#include <QPointer>
#include <QQuickItem>
#include <QQuickView>
#include <QScreen>
#include <QUrl>
#include <QVariant>

OverlayManager::OverlayManager(Configuration *configuration, QObject *parent)
    : QObject(parent)
    , m_configuration(configuration)
    , m_animationState(this)
{
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

    m_visible = true;
    m_animationEpochMs = QDateTime::currentMSecsSinceEpoch();
    updateAnimationState();
    const QList<QScreen *> screens = QGuiApplication::screens();
    for (QScreen *screen : screens) {
        addScreen(screen);
    }
    if (m_views.isEmpty()) {
        hide();
        return false;
    }
    // Install after the surfaces are created so window-mapping events cannot be
    // mistaken for user activity. Real input queued during creation is still
    // delivered after this synchronous method returns.
    qApp->installEventFilter(this);
    return true;
}

void OverlayManager::hide()
{
    if (!m_visible && m_views.isEmpty()) {
        return;
    }
    m_visible = false;
    m_animationState.stop();
    qApp->removeEventFilter(this);
    const auto views = m_views;
    m_views.clear();
    for (QQuickView *view : views) {
        view->hide();
        view->deleteLater();
    }
}

bool OverlayManager::isVisible() const
{
    return m_visible;
}

bool OverlayManager::addScreen(QScreen *screen)
{
    if (!screen || m_views.contains(screen)) {
        return screen != nullptr;
    }

    auto *view = new QQuickView;
    view->setObjectName(QStringLiteral("screensaver-%1").arg(screen->name()));
    view->setColor(Qt::black);
    view->setResizeMode(QQuickView::SizeRootObjectToView);
    view->setFlags(Qt::FramelessWindowHint | Qt::WindowStaysOnTopHint);
    view->setScreen(screen);
    view->setGeometry(screen->geometry());

    const uint seed = m_configuration->monitorBehavior() == QStringLiteral("synchronized")
        || m_configuration->monitorBehavior() == QStringLiteral("seamless")
        ? 1U : qHash(screen->name());
    const bool sharedMotion = m_configuration->monitorBehavior() != QStringLiteral("independent");
    const QRect screenGeometry = screen->geometry();
    const QRect virtualGeometry = screen->virtualGeometry();
    view->setInitialProperties({
        {QStringLiteral("visualModule"), m_configuration->visualModule()},
        {QStringLiteral("backgroundStyle"), m_configuration->backgroundStyle()},
        {QStringLiteral("animationSpeed"), m_configuration->animationSpeed()},
        {QStringLiteral("animationDensity"), m_configuration->animationDensity()},
        {QStringLiteral("animationScale"), m_configuration->animationScale()},
        {QStringLiteral("animationPalette"), m_configuration->animationPalette()},
        {QStringLiteral("trailAmount"), m_configuration->trailAmount()},
        {QStringLiteral("ballCount"), m_configuration->ballCount()},
        {QStringLiteral("ballGravity"), m_configuration->ballGravity()},
        {QStringLiteral("ballElasticity"), m_configuration->ballElasticity()},
        {QStringLiteral("ballCollisions"), m_configuration->ballCollisions()},
        {QStringLiteral("showClock"), m_configuration->showClock()},
        {QStringLiteral("clockMovement"), m_configuration->clockMovement()},
        {QStringLiteral("clockSpeed"), m_configuration->clockSpeed()},
        {QStringLiteral("frameRate"), m_configuration->frameRate()},
        {QStringLiteral("reducedMotion"), m_configuration->reducedMotion()},
        {QStringLiteral("monitorBehavior"), m_configuration->monitorBehavior()},
        {QStringLiteral("seed"), seed},
        {QStringLiteral("animationEpochMs"), sharedMotion
                                                ? m_animationEpochMs
                                                : QDateTime::currentMSecsSinceEpoch()},
        {QStringLiteral("screenX"), screenGeometry.x()},
        {QStringLiteral("screenY"), screenGeometry.y()},
        {QStringLiteral("virtualX"), virtualGeometry.x()},
        {QStringLiteral("virtualY"), virtualGeometry.y()},
        {QStringLiteral("virtualWidth"), virtualGeometry.width()},
        {QStringLiteral("virtualHeight"), virtualGeometry.height()},
        {QStringLiteral("animationState"), QVariant::fromValue(static_cast<QObject *>(&m_animationState))},
    });
    view->setSource(QUrl(QStringLiteral("qrc:/qml/Screensaver.qml")));
    if (view->status() == QQuickView::Error) {
        delete view;
        return false;
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
    m_views.insert(screen, view);
    view->show();
    updateAllViewGeometry();
    return true;
}

void OverlayManager::removeScreen(QScreen *screen)
{
    QQuickView *view = m_views.take(screen);
    if (view) {
        view->hide();
        view->deleteLater();
    }
    if (m_visible && m_views.isEmpty()) {
        Q_EMIT inputDetected();
    } else {
        updateAllViewGeometry();
    }
}

void OverlayManager::updateAllViewGeometry()
{
    updateAnimationState();
    const auto screens = m_views.keys();
    for (QScreen *screen : screens) {
        updateViewGeometry(screen);
    }
}

void OverlayManager::updateAnimationState()
{
    const bool seamless = m_configuration->monitorBehavior() == QStringLiteral("seamless");
    const bool motionAllowed = !m_configuration->reducedMotion();
    const bool animateBall = seamless && motionAllowed
        && m_configuration->visualModule() == QStringLiteral("bounce");
    const bool animateClock = seamless && motionAllowed && m_configuration->showClock()
        && m_configuration->clockMovement() == QStringLiteral("bounce");
    m_animationState.configure(QGuiApplication::screens(), animateBall, animateClock,
                               m_configuration->clockSpeed(), m_configuration->frameRate(),
                               m_configuration->ballCount(), m_configuration->animationSpeed(),
                               m_configuration->animationScale(), m_configuration->ballGravity(),
                               m_configuration->ballElasticity(), m_configuration->ballCollisions());
}

void OverlayManager::updateViewGeometry(QScreen *screen)
{
    QQuickView *view = m_views.value(screen);
    if (!view || !screen) {
        return;
    }
    const QRect screenGeometry = screen->geometry();
    const QRect virtualGeometry = screen->virtualGeometry();
    view->setScreen(screen);
    view->setGeometry(screenGeometry);
    if (QObject *root = view->rootObject()) {
        root->setProperty("screenX", screenGeometry.x());
        root->setProperty("screenY", screenGeometry.y());
        root->setProperty("virtualX", virtualGeometry.x());
        root->setProperty("virtualY", virtualGeometry.y());
        root->setProperty("virtualWidth", virtualGeometry.width());
        root->setProperty("virtualHeight", virtualGeometry.height());
    }
}

bool OverlayManager::eventFilter(QObject *watched, QEvent *event)
{
    Q_UNUSED(watched)
    if (m_visible && isDismissEvent(event)) {
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

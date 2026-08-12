// SPDX-License-Identifier: GPL-3.0-or-later
#include "overlaymanager.h"

#include "configuration.h"

#include <LayerShellQt/Window>
#include <QCoreApplication>
#include <QDateTime>
#include <QEvent>
#include <QGuiApplication>
#include <QPointer>
#include <QQuickView>
#include <QScreen>
#include <QUrl>

OverlayManager::OverlayManager(Configuration *configuration, QObject *parent)
    : QObject(parent)
    , m_configuration(configuration)
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
        ? 1U : qHash(screen->name());
    const bool synchronized = m_configuration->monitorBehavior() == QStringLiteral("synchronized");
    view->setInitialProperties({
        {QStringLiteral("visualModule"), m_configuration->visualModule()},
        {QStringLiteral("showClock"), m_configuration->showClock()},
        {QStringLiteral("frameRate"), m_configuration->frameRate()},
        {QStringLiteral("reducedMotion"), m_configuration->reducedMotion()},
        {QStringLiteral("seed"), seed},
        {QStringLiteral("animationEpochMs"), synchronized
                                                ? m_animationEpochMs
                                                : QDateTime::currentMSecsSinceEpoch()},
        {QStringLiteral("screenName"), screen->name()},
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
    layer->setExclusiveZone(0);
    layer->setKeyboardInteractivity(LayerShellQt::Window::KeyboardInteractivityExclusive);
    layer->setScope(QStringLiteral("plasma-visual-screensaver"));

    connect(screen, &QScreen::geometryChanged, view, [view, screen](const QRect &) {
        view->setScreen(screen);
        view->setGeometry(screen->geometry());
    });
    m_views.insert(screen, view);
    view->show();
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

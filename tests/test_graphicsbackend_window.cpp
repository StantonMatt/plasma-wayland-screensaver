// SPDX-License-Identifier: GPL-3.0-or-later
#include "graphicsbackend.h"
#include "presentationclock.h"
#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQuickView>
#include <QQuickWindow>
#include <QSignalSpy>
#include <QTest>
#include <QVulkanInstance>
#include <QThread>
#include <QScopeGuard>
#include <atomic>
#include <vector>

class GraphicsBackendWindowTest final : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void windows()
    {
        const bool requireVulkan = qEnvironmentVariableIsSet("PVS_TEST_VULKAN");
        if (requireVulkan) QCOMPARE(QQuickWindow::graphicsApi(), QSGRendererInterface::Vulkan);
        if (requireVulkan) QCOMPARE(qEnvironmentVariable("QSG_RENDER_LOOP"), QStringLiteral("threaded"));
        // This is the same creation path as each OverlayManager preview/output.
        QQuickView view;
        view.setSource(QUrl(QStringLiteral("data:text/plain,import QtQuick; Rectangle { width: 64; height: 64; color: 'green' }")));
        QCOMPARE(view.status(), QQuickView::Ready);
        view.create();
        auto *instance = view.vulkanInstance();
        if (requireVulkan) QVERIFY(instance && instance->isValid());
        else QVERIFY(!instance);
        if (requireVulkan) QCOMPARE(view.requestedFormat().swapInterval(), 0);

        // visible:true creates the native surface during QML load, before
        // QQmlApplicationEngine::objectCreated can attach a Vulkan instance.
        QQmlApplicationEngine engine;
        engine.loadData("import QtQuick; Window { width: 64; height: 64; visible: true; Rectangle { anchors.fill: parent; color: 'blue' } }");
        QCOMPARE(engine.rootObjects().size(), 1);
        auto *settings = qobject_cast<QQuickWindow *>(engine.rootObjects().constFirst());
        QVERIFY(settings);
        QCOMPARE(settings->vulkanInstance(), instance);
        if (requireVulkan) {
            QSignalSpy frames(settings, &QQuickWindow::frameSwapped);
            const bool swapped = frames.wait(5000); // wait() locks spy storage.
            const auto api = settings->rendererInterface()->graphicsApi();
            settings->setPersistentGraphics(false);
            settings->setPersistentSceneGraph(false);
            settings->hide();
            settings->releaseResources();
            // Quiesce before destroying the cross-thread spy, on failure too.
            QVERIFY(swapped);
            QCOMPARE(api, QSGRendererInterface::Vulkan);
        }
        view.destroy();
        view.create();
        QCOMPARE(view.vulkanInstance(), instance);
        // A newly created output after startup receives the same instance.
        QQuickWindow laterOutput;
        laterOutput.create();
        QCOMPARE(laterOutput.vulkanInstance(), instance);
    }
    void pacedWindows()
    {
        if (!qEnvironmentVariableIsSet("PVS_TEST_VULKAN")) QSKIP("Requires a Vulkan Wayland compositor");
        // Exercise real swap feedback for simultaneous independent windows.
        // A startup-only frame proves neither continued rendering nor pacing.
        constexpr int count = 3;
        QQuickView views[count];
        const auto renderedOnGuiThread = std::make_shared<std::atomic<bool>>(false);
        const auto *guiThread = QThread::currentThread();
        std::vector<std::unique_ptr<PresentationClock>> clocks;
        std::vector<std::unique_ptr<QSignalSpy>> frames;
        const auto stopViews = [&] {
            for (auto &clock : clocks) clock->setRunning(false);
            for (auto &view : views) {
                view.setPersistentGraphics(false);
                view.setPersistentSceneGraph(false);
                view.hide();
                view.releaseResources();
            }
        };
        // Also quiesce rendering before spies are destroyed on an early failure.
        auto cleanup = qScopeGuard(stopViews);
        for (int i = 0; i < count; ++i) {
            auto &view = views[i];
            const auto screens = QGuiApplication::screens();
            view.setScreen(screens[i % screens.size()]);
            view.setSource(QUrl(QStringLiteral("data:text/plain,import QtQuick; Rectangle { width: 64; height: 64; color: 'green' }")));
            QCOMPARE(view.status(), QQuickView::Ready);
            connect(&view, &QQuickWindow::beforeRendering, &view, [renderedOnGuiThread, guiThread] {
                if (QThread::currentThread() == guiThread) renderedOnGuiThread->store(true);
            }, Qt::DirectConnection);
            frames.push_back(std::make_unique<QSignalSpy>(&view, &QQuickWindow::frameSwapped));
            clocks.push_back(std::make_unique<PresentationClock>(&view, 60));
            view.show();
            clocks.back()->setRunning(true);
        }
        QTest::qWait(2000);
        // Stopping GUI timers alone leaves in-flight frameSwapped emissions.
        // Qt's hidden, nonpersistent resource release waits for render threads.
        // QSignalSpy's inherited QList reads are safe only after that barrier.
        stopViews();
        cleanup.dismiss();
        QVERIFY(!renderedOnGuiThread->load());
        for (const auto &spy : frames) {
            QVERIFY2(spy->size() >= 80, qPrintable(QStringLiteral("Only %1 swaps in 2s").arg(spy->size())));
            QVERIFY2(spy->size() <= 150, "Uncapped Vulkan submissions");
        }
    }
};

int main(int argc, char **argv)
{
    GraphicsBackend backend;
    QGuiApplication app(argc, argv);
    if (!backend.initialize(qEnvironmentVariable("PVS_GRAPHICS_API"))) return 2;
    GraphicsBackendWindowTest test;
    return QTest::qExec(&test, argc, argv);
}
#include "test_graphicsbackend_window.moc"

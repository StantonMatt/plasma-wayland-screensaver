// SPDX-License-Identifier: GPL-3.0-or-later
#include "graphicsbackend.h"
#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQuickView>
#include <QQuickWindow>
#include <QSignalSpy>
#include <QTest>
#include <QVulkanInstance>

class GraphicsBackendWindowTest final : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void windows()
    {
        const bool requireVulkan = qEnvironmentVariableIsSet("PVS_TEST_VULKAN");
        if (requireVulkan) QCOMPARE(QQuickWindow::graphicsApi(), QSGRendererInterface::Vulkan);
        // This is the same creation path as each OverlayManager preview/output.
        QQuickView view;
        view.setSource(QUrl(QStringLiteral("data:text/plain,import QtQuick; Rectangle { width: 64; height: 64; color: 'green' }")));
        QCOMPARE(view.status(), QQuickView::Ready);
        view.create();
        auto *instance = view.vulkanInstance();
        if (requireVulkan) QVERIFY(instance && instance->isValid());
        else QVERIFY(!instance);

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
            QVERIFY(frames.wait(5000));
            QCOMPARE(settings->rendererInterface()->graphicsApi(), QSGRendererInterface::Vulkan);
        }
        view.destroy();
        view.create();
        QCOMPARE(view.vulkanInstance(), instance);
        // A newly created output after startup receives the same instance.
        QQuickWindow laterOutput;
        laterOutput.create();
        QCOMPARE(laterOutput.vulkanInstance(), instance);
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

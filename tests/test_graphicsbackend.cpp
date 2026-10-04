// SPDX-License-Identifier: GPL-3.0-or-later
#include "graphicsbackend.h"
#include <QTest>

using GraphicsSelection::Api;
using GraphicsSelection::Inputs;

class GraphicsBackendTest final : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void policy_data()
    {
        QTest::addColumn<QString>("platform");
        QTest::addColumn<QString>("quick");
        QTest::addColumn<QString>("rhi");
        QTest::addColumn<QString>("pvs");
        QTest::addColumn<bool>("available");
        QTest::addColumn<int>("api");
        const auto row = [](const char *name, const char *platform, const char *quick,
                            const char *rhi, const char *pvs, bool available, Api api) {
            QTest::newRow(name) << QString::fromLatin1(platform) << QString::fromLatin1(quick)
                << QString::fromLatin1(rhi) << QString::fromLatin1(pvs) << available << int(api);
        };
        row("wayland", "wayland", "", "", "", true, Api::OpenGL);
        row("wayland-egl", "wayland-egl", "", "", "", true, Api::OpenGL);
        row("unavailable", "wayland", "", "", "", false, Api::OpenGL);
        row("offscreen", "offscreen", "", "", "", true, Api::OpenGL);
        row("minimal", "minimal", "", "", "", true, Api::OpenGL);
        row("xcb-default", "xcb", "", "", "", true, Api::OpenGL);
        row("software", "wayland", "software", "", "", true, Api::Software);
        row("software-rhi", "wayland", "software", "vulkan", "opengl", true, Api::Software);
        row("software-pvs", "offscreen", "software", "", "vulkan", true, Api::Software);
        row("custom-quick", "wayland", "custom", "opengl", "vulkan", true, Api::QtOverride);
        row("rhi-opengl", "wayland", "", "opengl", "", true, Api::OpenGL);
        row("rhi-precedence", "wayland", "", "opengl", "vulkan", true, Api::OpenGL);
        row("rhi-vulkan", "wayland", "", "vulkan", "opengl", true, Api::Vulkan);
        row("rhi-vulkan-fallback", "wayland", "", "vulkan", "opengl", false, Api::OpenGL);
        row("rhi-vulkan-offscreen", "offscreen", "", "vulkan", "", true, Api::OpenGL);
        row("rhi-vulkan-minimal", "minimal", "", "vulkan", "", true, Api::OpenGL);
        row("rhi-null", "wayland", "", "null", "vulkan", true, Api::QtOverride);
        row("rhi-unknown", "wayland", "", "custom", "", true, Api::QtOverride);
        row("pvs-opengl", "wayland", "", "", "opengl", true, Api::OpenGL);
        row("pvs-vulkan", "wayland", "", "", "vulkan", true, Api::Vulkan);
        row("pvs-vulkan-wayland-egl", "wayland-egl", "", "", "vulkan", true, Api::Vulkan);
        row("pvs-fallback", "wayland", "", "", "vulkan", false, Api::OpenGL);
        row("pvs-offscreen", "offscreen", "", "", "vulkan", true, Api::OpenGL);
        row("pvs-minimal", "minimal", "", "", "vulkan", true, Api::OpenGL);
        row("pvs-xcb", "xcb", "", "", "vulkan", true, Api::Vulkan);
        row("invalid-pvs", "wayland", "", "", "invalid", true, Api::Invalid);
        row("qt-precedes-invalid-pvs", "wayland", "software", "", "invalid", true, Api::Software);
    }
    void policy()
    {
        QFETCH(QString, platform);
        QFETCH(QString, quick);
        QFETCH(QString, rhi);
        QFETCH(QString, pvs);
        QFETCH(bool, available);
        QFETCH(int, api);
        QCOMPARE(int(GraphicsSelection::select(Inputs{platform, quick, rhi, pvs}, available)), api);
    }
};
QTEST_APPLESS_MAIN(GraphicsBackendTest)
#include "test_graphicsbackend.moc"

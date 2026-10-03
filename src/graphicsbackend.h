// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once

#include <QString>
#include <memory>

namespace GraphicsSelection {
enum class Api { OpenGL, Vulkan, Software, QtOverride, Invalid };
struct Inputs {
    QString platform;
    QString quickBackend;
    QString rhiBackend;
    QString requestedApi;
};
// Pure policy: the availability argument is supplied by the startup-only probe.
Api select(const Inputs &inputs, bool vulkanAvailable);
}

// Construct before QGuiApplication, initialize before any QQuickWindow, and
// destroy after QGuiApplication so the instance outlives every render thread.
class GraphicsBackend final
{
public:
    GraphicsBackend();
    ~GraphicsBackend();
    bool initialize(const QString &requestedApi);

private:
    struct Private;
    std::unique_ptr<Private> d;
};

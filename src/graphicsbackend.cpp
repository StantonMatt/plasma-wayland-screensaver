// SPDX-License-Identifier: GPL-3.0-or-later
#include "graphicsbackend.h"

#include <QDebug>
#include <QGuiApplication>
#include <QPlatformSurfaceEvent>
#include <QQuickGraphicsDevice>
#include <QQuickWindow>
#include <QSurfaceFormat>
#include <QVulkanFunctions>
#include <QVulkanInstance>
#include <QWindow>

#include <algorithm>
#include <cstring>
#include <vector>

using GraphicsSelection::Api;

Api GraphicsSelection::select(const Inputs &inputs, bool vulkanAvailable)
{
    // Qt's scene graph override has precedence over its RHI override. Leave
    // custom backends (and unknown Qt values) to Qt rather than interpreting them.
    if (!inputs.quickBackend.isEmpty()) {
        return inputs.quickBackend == QStringLiteral("software") ? Api::Software : Api::QtOverride;
    }
    const bool headless = inputs.platform == QStringLiteral("offscreen")
        || inputs.platform == QStringLiteral("minimal");
    if (!inputs.rhiBackend.isEmpty()) {
        if (inputs.rhiBackend == QStringLiteral("opengl")) return Api::OpenGL;
        if (inputs.rhiBackend == QStringLiteral("vulkan")) return vulkanAvailable && !headless ? Api::Vulkan : Api::OpenGL;
        return Api::QtOverride;
    }
    if (!inputs.requestedApi.isEmpty()
            && inputs.requestedApi != QStringLiteral("opengl")
            && inputs.requestedApi != QStringLiteral("vulkan")) return Api::Invalid;
    if (inputs.requestedApi == QStringLiteral("opengl")) return Api::OpenGL;
    // The offscreen/minimal plugins cannot present a Vulkan swapchain. Do not
    // initialize a Vulkan loader there, even for the PVS escape hatch.
    if (headless) return Api::OpenGL;
    if (inputs.requestedApi == QStringLiteral("vulkan")) return vulkanAvailable ? Api::Vulkan : Api::OpenGL;
    // Vulkan is experimental: presentation can stall on NVIDIA layer-shell
    // overlays even after a successful probe. Use it only on explicit request.
    return Api::OpenGL;
}

struct GraphicsBackend::Private : QObject {
    QVulkanInstance instance;
    VkPhysicalDevice physicalDevice = VK_NULL_HANDLE;

    bool eventFilter(QObject *watched, QEvent *event) override
    {
        if (event->type() == QEvent::PlatformSurface
                && static_cast<QPlatformSurfaceEvent *>(event)->surfaceEventType()
                    == QPlatformSurfaceEvent::SurfaceCreated) {
            if (auto *window = qobject_cast<QQuickWindow *>(watched)) {
                // Wayland creates VkSurface lazily, after this native-surface
                // event. Filter before Quick handles it or starts its RHI,
                // including visible QML ApplicationWindow roots. Recreated
                // windows retain their device; never reset an initialized RHI.
                if (window->vulkanInstance() != &instance) {
                    window->setVulkanInstance(&instance);
                    window->setGraphicsDevice(QQuickGraphicsDevice::fromPhysicalDevice(physicalDevice));
                }
            }
        }
        return false;
    }

    bool probe()
    {
        if (!instance.create()) return false;
        auto *functions = instance.functions();
        if (!functions) return false;
        uint32_t deviceCount = 0;
        if (functions->vkEnumeratePhysicalDevices(instance.vkInstance(), &deviceCount, nullptr) != VK_SUCCESS
                || !deviceCount) return false;
        std::vector<VkPhysicalDevice> devices(deviceCount);
        if (functions->vkEnumeratePhysicalDevices(instance.vkInstance(), &deviceCount, devices.data()) != VK_SUCCESS) return false;
        devices.resize(deviceCount);

        // A hidden native window obtains the actual platform's Vulkan surface;
        // no QQuickWindow, logical device, swapchain or rendering is needed.
        QWindow surface;
        surface.setSurfaceType(QSurface::VulkanSurface);
        surface.setVulkanInstance(&instance);
        surface.create();
        const VkSurfaceKHR handle = QVulkanInstance::surfaceForWindow(&surface);
        if (!handle) return false;
        const auto getFormats = reinterpret_cast<PFN_vkGetPhysicalDeviceSurfaceFormatsKHR>(
            instance.getInstanceProcAddr("vkGetPhysicalDeviceSurfaceFormatsKHR"));
        const auto getModes = reinterpret_cast<PFN_vkGetPhysicalDeviceSurfacePresentModesKHR>(
            instance.getInstanceProcAddr("vkGetPhysicalDeviceSurfacePresentModesKHR"));
        if (!getFormats || !getModes) return false;

        // Prefer hardware over a CPU ICD; pin each Quick window to the device
        // actually probed so RHI cannot choose a different, non-presenting GPU.
        std::stable_sort(devices.begin(), devices.end(), [functions](auto a, auto b) {
            VkPhysicalDeviceProperties pa{}, pb{};
            functions->vkGetPhysicalDeviceProperties(a, &pa);
            functions->vkGetPhysicalDeviceProperties(b, &pb);
            const auto rank = [](VkPhysicalDeviceType type) {
                return type == VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU ? 0
                    : type == VK_PHYSICAL_DEVICE_TYPE_INTEGRATED_GPU ? 1
                    : type == VK_PHYSICAL_DEVICE_TYPE_CPU ? 3 : 2;
            };
            return rank(pa.deviceType) < rank(pb.deviceType);
        });
        for (const auto device : devices) {
            uint32_t extensionCount = 0;
            if (functions->vkEnumerateDeviceExtensionProperties(device, nullptr, &extensionCount, nullptr) != VK_SUCCESS) continue;
            std::vector<VkExtensionProperties> extensions(extensionCount);
            if (functions->vkEnumerateDeviceExtensionProperties(device, nullptr, &extensionCount, extensions.data()) != VK_SUCCESS) continue;
            extensions.resize(extensionCount);
            if (std::none_of(extensions.begin(), extensions.end(), [](const auto &extension) {
                return std::strcmp(extension.extensionName, VK_KHR_SWAPCHAIN_EXTENSION_NAME) == 0;
            })) continue;
            uint32_t formatCount = 0, modeCount = 0;
            if (getFormats(device, handle, &formatCount, nullptr) != VK_SUCCESS || !formatCount
                    || getModes(device, handle, &modeCount, nullptr) != VK_SUCCESS || !modeCount) continue;
            std::vector<VkPresentModeKHR> modes(modeCount);
            if (getModes(device, handle, &modeCount, modes.data()) != VK_SUCCESS) continue;
            modes.resize(modeCount);
            // FIFO can block each output independently. Require the modes
            // QRhi uses for NoVSync rather than silently falling back to FIFO.
            if (std::find(modes.begin(), modes.end(), VK_PRESENT_MODE_MAILBOX_KHR) == modes.end()
                    && std::find(modes.begin(), modes.end(), VK_PRESENT_MODE_IMMEDIATE_KHR) == modes.end()) continue;
            uint32_t queueCount = 0;
            functions->vkGetPhysicalDeviceQueueFamilyProperties(device, &queueCount, nullptr);
            std::vector<VkQueueFamilyProperties> queues(queueCount);
            functions->vkGetPhysicalDeviceQueueFamilyProperties(device, &queueCount, queues.data());
            for (uint32_t i = 0; i < queueCount; ++i) {
                if (queues[i].queueCount && (queues[i].queueFlags & VK_QUEUE_GRAPHICS_BIT)
                        && instance.supportsPresent(device, i, &surface)) {
                    physicalDevice = device;
                    return true;
                }
            }
        }
        return false;
    }
};

GraphicsBackend::GraphicsBackend() = default;
GraphicsBackend::~GraphicsBackend() = default;

bool GraphicsBackend::initialize(const QString &requestedApi)
{
    const GraphicsSelection::Inputs inputs{QGuiApplication::platformName(),
        qEnvironmentVariable("QT_QUICK_BACKEND"), qEnvironmentVariable("QSG_RHI_BACKEND"), requestedApi};
    Api api = GraphicsSelection::select(inputs, true);
    if (api == Api::Invalid) {
        qCritical("PVS_GRAPHICS_API/--graphics-api must be opengl or vulkan");
        return false;
    }
    bool fallback = false;
    if (api == Api::Vulkan) {
        d = std::make_unique<Private>();
        if (!d->probe()) {
            d.reset();
            api = GraphicsSelection::select(inputs, false);
            fallback = true;
        }
    }
    QString name;
    switch (api) {
    case Api::Vulkan:
        // QtWayland's Vulkan presentAboutToBeQueued() waits for a frame
        // callback when swapInterval > 0. On the basic loop those per-output
        // waits block the GUI thread, including our pacing timers. Qt 6.10
        // maps interval 0 to QRhiSwapChain::NoVSync (MAILBOX, then IMMEDIATE)
        // and skips that Wayland wait. PresentationClock still caps updates.
        // Do this before any Quick window, including visible QML roots.
        {
            auto format = QSurfaceFormat::defaultFormat();
            format.setSwapInterval(0);
            QSurfaceFormat::setDefaultFormat(format);
        }
        // Each output needs an independent RHI/render thread; inherited Qt
        // loop overrides must not reintroduce GUI-thread Vulkan presentation.
        qputenv("QSG_RENDER_LOOP", "threaded");
        QQuickWindow::setGraphicsApi(QSGRendererInterface::Vulkan);
        qInfo("PVS Vulkan presentation: threaded render loop, swap interval 0 (MAILBOX/IMMEDIATE)");
        name = QStringLiteral("vulkan");
        break;
    case Api::OpenGL:
        QQuickWindow::setGraphicsApi(QSGRendererInterface::OpenGL);
        name = QStringLiteral("opengl");
        break;
    case Api::Software: name = QStringLiteral("software (QT_QUICK_BACKEND)"); break;
    case Api::QtOverride:
        name = QStringLiteral("Qt override (%1=%2)").arg(
            inputs.quickBackend.isEmpty() ? QStringLiteral("QSG_RHI_BACKEND") : QStringLiteral("QT_QUICK_BACKEND"),
            inputs.quickBackend.isEmpty() ? inputs.rhiBackend : inputs.quickBackend);
        break;
    case Api::Invalid: return false;
    }
    if (d) qGuiApp->installEventFilter(d.get());
    qInfo().noquote() << QStringLiteral("PVS graphics backend: %1%2").arg(name,
        fallback ? QStringLiteral(" (Vulkan presentation unavailable)") : QString());
    return true;
}

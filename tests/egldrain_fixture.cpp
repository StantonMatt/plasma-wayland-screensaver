// SPDX-License-Identifier: GPL-3.0-or-later
// Loaded from an isolated test-only libnvidia-egl-wayland2.so.1.0.1. It creates
// named queues like upstream, but never loads EGL or accesses a GPU.
#include <dlfcn.h>
struct wl_display;
struct wl_event_queue;
extern "C" __attribute__((visibility("default")))
wl_event_queue *fixtureCreate(wl_display *display, const char *name)
{
    using Create = wl_event_queue *(*)(wl_display *, const char *);
    const auto create = reinterpret_cast<Create>(dlsym(RTLD_DEFAULT, "wl_display_create_queue_with_name"));
    if (!create) return nullptr;
    auto *result = create(display, name);
    // Preserve this DSO as the return-address caller even in Release builds.
    asm volatile("" : "+r"(result));
    return result;
}

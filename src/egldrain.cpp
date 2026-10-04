// SPDX-License-Identifier: GPL-3.0-or-later
#include "egldrain.h"

#include <algorithm>
#include <atomic>
#include <cerrno>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <dlfcn.h>
#include <limits.h>
#include <mutex>
#include <vector>

// Opaque public Wayland ABI types. No dependency on Wayland development
// headers or an eagerly loaded EGL/Wayland library is needed.
struct wl_display;
struct wl_event_queue;

namespace {
std::atomic<bool> enabled{false};
char selectedLibrary[PATH_MAX]{};
const char *selectedVersion = nullptr;
std::atomic<bool> unexpectedCallerReported{false};

struct Functions {
    using Create = wl_event_queue *(*)(wl_display *);
    using CreateNamed = wl_event_queue *(*)(wl_display *, const char *);
    using Dispatch = int (*)(wl_display *, wl_event_queue *);
    using Destroy = void (*)(wl_event_queue *);
    using Disconnect = void (*)(wl_display *);
    Create create = reinterpret_cast<Create>(dlsym(RTLD_NEXT, "wl_display_create_queue"));
    CreateNamed createNamed = reinterpret_cast<CreateNamed>(dlsym(RTLD_NEXT, "wl_display_create_queue_with_name"));
    Dispatch dispatch = reinterpret_cast<Dispatch>(dlsym(RTLD_NEXT, "wl_display_dispatch_queue_pending"));
    Destroy destroy = reinterpret_cast<Destroy>(dlsym(RTLD_NEXT, "wl_event_queue_destroy"));
    Disconnect disconnect = reinterpret_cast<Disconnect>(dlsym(RTLD_NEXT, "wl_display_disconnect"));
};

const Functions &functions()
{
    static const Functions value;
    return value;
}

struct Entry {
    wl_display *display;
    wl_event_queue *queue;
    uint32_t surface;
    bool swapchain;
    bool reported = false;
};

struct Registry {
    std::mutex mutex;
    std::vector<Entry> entries; // Only queue creation can allocate.
    ~Registry() { enabled.store(false, std::memory_order_relaxed); }
};

Registry &registry()
{
    // Function-local initialization also makes calls from DSO constructors
    // safe. Nothing is registered until enable() runs at the start of main.
    static Registry value;
    return value;
}

bool supportedCaller(void *caller)
{
    Dl_info info{};
    if (!dladdr(caller, &info) || !info.dli_fname) return false;
    // dladdr may return the SONAME symlink rather than the versioned file.
    char resolved[PATH_MAX];
    if (!realpath(info.dli_fname, resolved)) return false;
    const char *base = std::strrchr(resolved, '/');
    return base && EglDrain::supportedVersion(resolved) && std::strcmp(resolved, selectedLibrary) == 0;
}

bool parseName(const char *name, uint32_t &surface, bool &swapchain)
{
    constexpr char prefix[] = "EGLSurface(";
    if (!name || std::strncmp(name, prefix, sizeof(prefix) - 1) != 0) return false;
    const char *p = name + sizeof(prefix) - 1;
    uint64_t id = 0;
    if (*p < '0' || *p > '9') return false;
    while (*p >= '0' && *p <= '9') {
        id = id * 10 + unsigned(*p++ - '0');
        if (id > UINT32_MAX) return false;
    }
    if (id == 0) return false;
    swapchain = *p == '/';
    if (swapchain) {
        ++p;
        // glibc snprintf("%p") for the non-null WlSwapChain pointer.
        if (p[0] != '0' || p[1] != 'x') return false;
        p += 2;
        const char *start = p;
        while ((*p >= '0' && *p <= '9') || (*p >= 'a' && *p <= 'f') || (*p >= 'A' && *p <= 'F')) ++p;
        if (p == start) return false;
    }
    if (*p != ')' || p[1] != '\0') return false;
    surface = uint32_t(id);
    return true;
}

wl_event_queue *matchingSwapchain(wl_display *display, wl_event_queue *queue)
{
    auto &r = registry();
    std::lock_guard lock(r.mutex);
    const auto surface = std::find_if(r.entries.begin(), r.entries.end(), [=](const Entry &e) {
        return e.display == display && e.queue == queue && !e.swapchain;
    });
    if (surface == r.entries.end()) return nullptr;
    Entry *match = nullptr;
    for (auto &e : r.entries) {
        if (e.display == display && e.surface == surface->surface && e.swapchain) {
            // A swap starts with exactly one live swapchain in the verified sources. Fail
            // closed if a different implementation violates that contract.
            if (match) return nullptr;
            match = &e;
        }
    }
    if (!match) return nullptr;
    if (!match->reported) {
        std::fprintf(stderr, "PVS EGL drain: active wl_surface=%u (egl-wayland2 %s)\n", match->surface, selectedVersion);
        match->reported = true;
        surface->reported = true;
    }
    return match->queue;
}
}

const char *EglDrain::supportedVersion(const char *canonicalPath)
{
    if (!canonicalPath) return nullptr;
    const char *base = std::strrchr(canonicalPath, '/');
    if (!base) return nullptr;
    // Source checked: v1.0.1 c757f0f, v1.0.2 5a2c1cb, main ce0eb711
    // (meson version 1.0.3). Queue creation/names/dispatch are unchanged.
    for (const char *version : {"1.0.1", "1.0.2", "1.0.3"}) {
        constexpr char prefix[] = "libnvidia-egl-wayland2.so.";
        if (std::strncmp(base + 1, prefix, sizeof(prefix) - 1) == 0
                && std::strcmp(base + sizeof(prefix), version) == 0) return version;
    }
    return nullptr;
}

bool EglDrain::enable(const char *canonicalPath)
{
    const char *version = supportedVersion(canonicalPath);
    if (!version || std::strlen(canonicalPath) >= sizeof(selectedLibrary)) return false;
    Dl_info self{};
    if (!dladdr(reinterpret_cast<void *>(&EglDrain::enable), &self)) return false;
    // Verify the executable exports and interposition precedence before EGL
    // can cache RTLD_DEFAULT's named-queue entry point. Never load EGL here.
    for (const char *name : {"wl_display_create_queue_with_name", "wl_display_dispatch_queue_pending",
                            "wl_event_queue_destroy", "wl_display_disconnect"}) {
        Dl_info symbol{};
        if (!dladdr(dlsym(RTLD_DEFAULT, name), &symbol) || symbol.dli_fbase != self.dli_fbase) return false;
    }
    // Called once before EGL / render threads start; immutable thereafter.
    std::strcpy(selectedLibrary, canonicalPath);
    selectedVersion = version;
    enabled.store(true, std::memory_order_relaxed);
    return true;
}

// Explicit export is needed with KDE's hidden symbol visibility. The CMake
// interface adds these symbols to the executable's dynamic symbol table.
extern "C" __attribute__((visibility("default")))
wl_event_queue *wl_display_create_queue_with_name(wl_display *display, const char *name)
{
    const auto &f = functions();
    // egl-wayland2 discovers this entry point via dlsym(RTLD_DEFAULT). On old
    // libwayland our export must still create a valid queue, not a null one.
    if (!f.createNamed && !f.create) { errno = ENOSYS; return nullptr; }
    wl_event_queue *queue = f.createNamed ? f.createNamed(display, name) : f.create(display);
    const int savedErrno = errno;
    if (queue && enabled.load(std::memory_order_relaxed)) {
        uint32_t surface;
        bool swapchain;
        if (parseName(name, surface, swapchain)
                && supportedCaller(__builtin_extract_return_addr(__builtin_return_address(0)))) {
            auto &r = registry();
            std::lock_guard lock(r.mutex);
            r.entries.push_back({display, queue, surface, swapchain});
        } else if (name && std::strncmp(name, "EGLSurface(", 11) == 0
                   && !unexpectedCallerReported.exchange(true, std::memory_order_relaxed)) {
            std::fprintf(stderr, "PVS EGL drain: inactive: unexpected EGL queue name/caller (%s); "
                         "cannot change explicit sync after EGL initialization; restart with PVS_EGL_LEAK_FIX=noexplicit\n", name);
        }
    }
    errno = savedErrno;
    return queue;
}

extern "C" __attribute__((visibility("default")))
int wl_display_dispatch_queue_pending(wl_display *display, wl_event_queue *queue)
{
    const auto dispatch = functions().dispatch;
    if (!dispatch) { errno = ENOSYS; return -1; }
    if (enabled.load(std::memory_order_relaxed)) {
        // This is egl-wayland2's existing eplWlSwapBuffers dispatch of
        // current.queue, BEFORE SwapChainRealloc. Match by display + surface
        // id, never by creation thread: EGL surfaces migrate to render threads.
        // The verified versions' current data/swapchain is exclusive to the current EGL
        // thread. The base swap hook also holds the surface-list read lock;
        // destruction takes its write lock, protecting the copied pointer.
        // Creation roundtrips have completed; release has no listener
        // under explicit sync. libwayland simply frees those closures.
        // No registry lock is held while dispatching (or invoking listeners).
        if (auto *swapchain = matchingSwapchain(display, queue)) {
            if (dispatch(display, swapchain) < 0) return -1;
        }
    }
    return dispatch(display, queue);
}

extern "C" __attribute__((visibility("default")))
void wl_event_queue_destroy(wl_event_queue *queue)
{
    if (enabled.load(std::memory_order_relaxed)) {
        auto &r = registry();
        std::lock_guard lock(r.mutex);
        for (const auto &e : r.entries) {
            if (e.queue == queue && !e.swapchain && !e.reported) {
                std::fprintf(stderr, "PVS EGL drain: never active wl_surface=%u; if this surface rendered, "
                             "restart with PVS_EGL_LEAK_FIX=noexplicit\n", e.surface);
            }
        }
        std::erase_if(r.entries, [=](const Entry &e) { return e.queue == queue; });
    }
    if (const auto destroy = functions().destroy) destroy(queue);
}

extern "C" __attribute__((visibility("default")))
void wl_display_disconnect(wl_display *display)
{
    if (enabled.load(std::memory_order_relaxed)) {
        auto &r = registry();
        std::lock_guard lock(r.mutex);
        // egl-wayland2 skips queue_destroy when the native display has gone
        // away. libwayland frees its queues here; forget all their addresses
        // before a later display can reuse them.
        std::erase_if(r.entries, [=](const Entry &e) { return e.display == display; });
    }
    if (const auto disconnect = functions().disconnect) disconnect(display);
}

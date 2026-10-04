// SPDX-License-Identifier: GPL-3.0-or-later
#include "egldrain.h"
#include <QLibrary>
#include <QTest>
#include <cerrno>
#include <cstdlib>
#include <dlfcn.h>
#include <new>
#include <sys/socket.h>
#include <thread>
#include <unistd.h>

namespace {
thread_local bool countAllocations = false;
thread_local size_t allocations = 0;
}
void *operator new(size_t size)
{
    if (countAllocations) ++allocations;
    if (void *p = std::malloc(size ? size : 1)) return p;
    std::abort(); // This project's KDE toolchain builds without exceptions.
}
void *operator new[](size_t size) { return ::operator new(size); }
void operator delete(void *p) noexcept { std::free(p); }
void operator delete[](void *p) noexcept { std::free(p); }
void operator delete(void *p, size_t) noexcept { std::free(p); }
void operator delete[](void *p, size_t) noexcept { std::free(p); }

struct wl_display;
struct wl_event_queue;
struct wl_proxy;
struct wl_interface;
extern "C" {
extern const wl_interface wl_buffer_interface;
wl_display *wl_display_connect_to_fd(int);
void wl_display_disconnect(wl_display *);
wl_event_queue *wl_display_create_queue_with_name(wl_display *, const char *);
void wl_event_queue_destroy(wl_event_queue *);
int wl_display_dispatch_queue_pending(wl_display *, wl_event_queue *);
int wl_display_prepare_read(wl_display *);
int wl_display_read_events(wl_display *);
wl_proxy *wl_proxy_create(wl_proxy *, const wl_interface *);
void wl_proxy_destroy(wl_proxy *);
void wl_proxy_set_queue(wl_proxy *, wl_event_queue *);
uint32_t wl_proxy_get_id(wl_proxy *);
int wl_proxy_add_listener(wl_proxy *, void (**)(void), void *);
}

class EglDrainTest final : public QObject
{
    Q_OBJECT
    using Create = wl_event_queue *(*)(wl_display *, const char *);
    using Dispatch = int (*)(wl_display *, wl_event_queue *);
    QLibrary fixture{QString::fromLocal8Bit(EGL_DRAIN_FIXTURE)};
    Create create = nullptr;
    Dispatch realDispatch = nullptr;
    wl_display *display = nullptr;
    int server = -1;

    void release(wl_proxy *buffer)
    {
        // Minimal compositor wire message: wl_buffer.release, no arguments.
        const uint32_t message[]{wl_proxy_get_id(buffer), 8U << 16};
        QCOMPARE(write(server, message, sizeof(message)), ssize_t(sizeof(message)));
        QCOMPARE(wl_display_prepare_read(display), 0);
        QCOMPARE(wl_display_read_events(display), 0);
    }

    wl_proxy *bufferOn(wl_event_queue *queue)
    {
        auto *buffer = wl_proxy_create(reinterpret_cast<wl_proxy *>(display), &wl_buffer_interface);
        if (buffer) wl_proxy_set_queue(buffer, queue);
        return buffer;
    }

private Q_SLOTS:
    void initTestCase()
    {
        QVERIFY2(fixture.load(), qPrintable(fixture.errorString()));
        create = reinterpret_cast<Create>(fixture.resolve("fixtureCreate"));
        QVERIFY(create);
        // Bypass our exported entry point for assertions on the actual queue.
        realDispatch = reinterpret_cast<Dispatch>(dlsym(RTLD_NEXT, "wl_display_dispatch_queue_pending"));
        QVERIFY(realDispatch);
    }
    void init()
    {
        // Enable before queue creation in every drain test, including focused
        // single-function invocations. The first test checks startup default.
        if (qstrcmp(QTest::currentTestFunction(), "disabledIsTransparent") != 0) QVERIFY(EglDrain::enable(EGL_DRAIN_FIXTURE));
        int sockets[2];
        QCOMPARE(socketpair(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0, sockets), 0);
        display = wl_display_connect_to_fd(sockets[0]);
        QVERIFY(display);
        server = sockets[1];
    }
    void cleanup()
    {
        wl_display_disconnect(display);
        close(server);
        display = nullptr;
        server = -1;
    }
    void disabledIsTransparent()
    {
        auto *surface = create(display, "EGLSurface(20)");
        auto *swapchain = create(display, "EGLSurface(20/0xabc)");
        QVERIFY(surface && swapchain);
        auto *buffer = bufferOn(swapchain);
        QVERIFY(buffer);
        release(buffer);
        QCOMPARE(wl_display_dispatch_queue_pending(display, surface), 0);
        QCOMPARE(realDispatch(display, swapchain), 1); // Undrained without opt-in.
        wl_proxy_destroy(buffer);
        wl_event_queue_destroy(swapchain);
        wl_event_queue_destroy(surface);
        QVERIFY(EglDrain::enable(EGL_DRAIN_FIXTURE)); // Enabled for all following tests, as at startup.
    }
    void matchingSurfaceOnlyAndThreadMigration()
    {
        auto *surface = create(display, "EGLSurface(20)");
        auto *swapchain = create(display, "EGLSurface(20/0xabc)");
        auto *other = create(display, "EGLSurface(21/0xdef)");
        QVERIFY(surface && swapchain && other);
        auto *buffer = bufferOn(swapchain);
        auto *otherBuffer = bufferOn(other);
        QVERIFY(buffer && otherBuffer);
        release(buffer);
        release(otherBuffer);
        int result = -1;
        std::thread render([&] { result = wl_display_dispatch_queue_pending(display, surface); });
        render.join();
        QCOMPARE(result, 0); // Retains the original surface dispatch return.
        QCOMPARE(realDispatch(display, swapchain), 0); // Real no-listener release freed.
        QCOMPARE(realDispatch(display, other), 1); // Other surface untouched.
        // Steady-state path, with real Wayland closures read and freed each time.
        for (int i = 0; i < 1000; ++i) {
            release(buffer);
            QCOMPARE(wl_display_dispatch_queue_pending(display, surface), 0);
            QCOMPARE(realDispatch(display, swapchain), 0);
        }
        wl_proxy_destroy(buffer);
        wl_proxy_destroy(otherBuffer);
        wl_event_queue_destroy(other);
        wl_event_queue_destroy(swapchain);
        wl_event_queue_destroy(surface);
    }
    void untrustedCallerAndMalformedNames()
    {
        for (const char *name : {"EGLSurface(0)", "EGLSurface(4294967296)", "EGLSurface(20)junk",
                                  "EGLSurface(20/0x)", "EGLSurface(20/garbage)", "unrelated"}) {
            auto *surface = create(display, "EGLSurface(20)");
            auto *queue = create(display, name);
            QVERIFY(surface && queue);
            auto *buffer = bufferOn(queue);
            QVERIFY(buffer);
            release(buffer);
            QCOMPARE(wl_display_dispatch_queue_pending(display, surface), 0);
            QCOMPARE(realDispatch(display, queue), 1);
            wl_proxy_destroy(buffer);
            wl_event_queue_destroy(queue);
            wl_event_queue_destroy(surface);
        }
        auto *surface = create(display, "EGLSurface(20)");
        // Same name from the executable/Qt is deliberately not registered.
        auto *queue = wl_display_create_queue_with_name(display, "EGLSurface(20/0xabc)");
        auto *buffer = bufferOn(queue);
        QVERIFY(surface && queue && buffer);
        release(buffer);
        QCOMPARE(wl_display_dispatch_queue_pending(display, surface), 0);
        QCOMPARE(realDispatch(display, queue), 1);
        wl_proxy_destroy(buffer);
        wl_event_queue_destroy(queue);
        wl_event_queue_destroy(surface);
    }
    void recreationAndListenerSemantics()
    {
        auto *surface = create(display, "EGLSurface(20)");
        auto *old = create(display, "EGLSurface(20/0xabc)");
        QVERIFY(surface && old);
        wl_event_queue_destroy(old);
        auto *replacement = create(display, "EGLSurface(20/0xdef)");
        QVERIFY(replacement);
        auto *buffer = bufferOn(replacement);
        QVERIFY(buffer);
        int calls = 0;
        // If explicit sync is unavailable, existing release listeners are
        // invoked normally on the swapping thread, before buffer-list use.
        struct Listener { void (*release)(void *, wl_proxy *); };
        const Listener listener{[](void *data, wl_proxy *) { ++*static_cast<int *>(data); }};
        QCOMPARE(wl_proxy_add_listener(buffer, reinterpret_cast<void (**)(void)>(const_cast<Listener *>(&listener)), &calls), 0);
        release(buffer);
        QCOMPARE(wl_display_dispatch_queue_pending(display, surface), 0);
        QCOMPARE(calls, 1);
        QCOMPARE(realDispatch(display, replacement), 0);
        wl_proxy_destroy(buffer);
        wl_event_queue_destroy(replacement);
        wl_event_queue_destroy(surface);
    }
    void verifiedVersions_data()
    {
        QTest::addColumn<QString>("path");
        QTest::newRow("1.0.2") << QString::fromLocal8Bit(EGL_DRAIN_FIXTURE_102);
        QTest::newRow("1.0.3-main") << QString::fromLocal8Bit(EGL_DRAIN_FIXTURE_103);
    }
    void verifiedVersions()
    {
        QFETCH(QString, path);
        QLibrary supported(path);
        QVERIFY2(supported.load(), qPrintable(supported.errorString()));
        QVERIFY(EglDrain::enable(path.toLocal8Bit().constData()));
        auto otherCreate = reinterpret_cast<Create>(supported.resolve("fixtureCreate"));
        QVERIFY(otherCreate);
        auto *surface = otherCreate(display, "EGLSurface(20)");
        auto *swapchain = otherCreate(display, "EGLSurface(20/0xabc)");
        QVERIFY(surface && swapchain);
        auto *buffer = bufferOn(swapchain);
        QVERIFY(buffer);
        release(buffer);
        QCOMPARE(wl_display_dispatch_queue_pending(display, surface), 0);
        QCOMPARE(realDispatch(display, swapchain), 0);
        wl_proxy_destroy(buffer);
        wl_event_queue_destroy(swapchain);
        wl_event_queue_destroy(surface);
    }
    void differentVerifiedLibraryIsTransparent()
    {
        QLibrary supported(QString::fromLocal8Bit(EGL_DRAIN_FIXTURE_102));
        QVERIFY(supported.load());
        auto otherCreate = reinterpret_cast<Create>(supported.resolve("fixtureCreate"));
        QVERIFY(otherCreate);
        auto *surface = otherCreate(display, "EGLSurface(20)");
        auto *swapchain = otherCreate(display, "EGLSurface(20/0xabc)");
        QVERIFY(surface && swapchain);
        auto *buffer = bufferOn(swapchain);
        QVERIFY(buffer);
        release(buffer);
        QCOMPARE(wl_display_dispatch_queue_pending(display, surface), 0);
        QCOMPARE(realDispatch(display, swapchain), 1);
        wl_proxy_destroy(buffer);
        wl_event_queue_destroy(swapchain);
        wl_event_queue_destroy(surface);
    }
    void unsupportedVersionIsTransparent()
    {
        QVERIFY(!EglDrain::enable(EGL_DRAIN_UNSUPPORTED_FIXTURE));
        QLibrary unsupported{QString::fromLocal8Bit(EGL_DRAIN_UNSUPPORTED_FIXTURE)};
        QVERIFY2(unsupported.load(), qPrintable(unsupported.errorString()));
        auto otherCreate = reinterpret_cast<Create>(unsupported.resolve("fixtureCreate"));
        QVERIFY(otherCreate);
        auto *surface = otherCreate(display, "EGLSurface(20)");
        auto *swapchain = otherCreate(display, "EGLSurface(20/0xabc)");
        QVERIFY(surface && swapchain);
        auto *buffer = bufferOn(swapchain);
        QVERIFY(buffer);
        release(buffer);
        QCOMPARE(wl_display_dispatch_queue_pending(display, surface), 0);
        QCOMPARE(realDispatch(display, swapchain), 1);
        wl_proxy_destroy(buffer);
        wl_event_queue_destroy(swapchain);
        wl_event_queue_destroy(surface);
    }
    void ambiguousSwapchainsAreNotDrained()
    {
        auto *surface = create(display, "EGLSurface(20)");
        auto *one = create(display, "EGLSurface(20/0xabc)");
        auto *two = create(display, "EGLSurface(20/0xdef)");
        QVERIFY(surface && one && two);
        auto *buffer = bufferOn(one);
        QVERIFY(buffer);
        release(buffer);
        QCOMPARE(wl_display_dispatch_queue_pending(display, surface), 0);
        QCOMPARE(realDispatch(display, one), 1);
        wl_proxy_destroy(buffer);
        wl_event_queue_destroy(two);
        wl_event_queue_destroy(one);
        wl_event_queue_destroy(surface);
    }
    void displayErrorsPropagate()
    {
        auto *surface = create(display, "EGLSurface(20)");
        auto *swapchain = create(display, "EGLSurface(20/0xabc)");
        QVERIFY(surface && swapchain);
        QCOMPARE(wl_display_prepare_read(display), 0);
        shutdown(server, SHUT_RDWR);
        QCOMPARE(wl_display_read_events(display), -1);
        errno = 0;
        QCOMPARE(wl_display_dispatch_queue_pending(display, surface), -1);
        QCOMPARE(errno, EPIPE);
        wl_event_queue_destroy(swapchain);
        wl_event_queue_destroy(surface);
    }
    void disconnectedDisplaysForgetQueues()
    {
        // Mirror native-display teardown: EGL may skip explicit queue_destroy.
        // No proxies here, so no abandoned protocol objects or warnings.
        auto *surface = create(display, "EGLSurface(20)");
        auto *swapchain = create(display, "EGLSurface(20/0xabc)");
        QVERIFY(surface && swapchain);
        wl_display_disconnect(display);
        close(server);
        int sockets[2];
        QCOMPARE(socketpair(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0, sockets), 0);
        display = wl_display_connect_to_fd(sockets[0]);
        server = sockets[1];
        QVERIFY(display);
        surface = create(display, "EGLSurface(20)");
        swapchain = create(display, "EGLSurface(20/0xdef)");
        QVERIFY(surface && swapchain);
        auto *buffer = bufferOn(swapchain);
        QVERIFY(buffer);
        release(buffer);
        QCOMPARE(wl_display_dispatch_queue_pending(display, surface), 0);
        QCOMPARE(realDispatch(display, swapchain), 0);
        wl_proxy_destroy(buffer);
        wl_event_queue_destroy(swapchain);
        wl_event_queue_destroy(surface);
    }
    void steadyDispatchDoesNotAllocate()
    {
        auto *surface = create(display, "EGLSurface(20)");
        auto *swapchain = create(display, "EGLSurface(20/0xabc)");
        QVERIFY(surface && swapchain);
        auto *buffer = bufferOn(swapchain);
        QVERIFY(buffer);
        // Warm the one-time activation log and all lazy symbol resolution.
        QCOMPARE(wl_display_dispatch_queue_pending(display, surface), 0);
        allocations = 0;
        int failures = 0;
        for (int i = 0; i < 1000; ++i) {
            release(buffer); // Reading allocates a closure; not part of drain.
            countAllocations = true;
            failures += wl_display_dispatch_queue_pending(display, surface) != 0;
            countAllocations = false;
        }
        QCOMPARE(failures, 0);
        QCOMPARE(allocations, size_t(0));
        QCOMPARE(realDispatch(display, swapchain), 0);
        wl_proxy_destroy(buffer);
        wl_event_queue_destroy(swapchain);
        wl_event_queue_destroy(surface);
    }
    void benchmarkEmptyDispatch_data()
    {
        QTest::addColumn<bool>("drain");
        QTest::newRow("original") << false;
        QTest::newRow("drain") << true;
    }
    void benchmarkEmptyDispatch()
    {
        QFETCH(bool, drain);
        // When selected alone for a focused benchmark, earlier tests need
        // not have run the startup-style enable.
        QVERIFY(EglDrain::enable(EGL_DRAIN_FIXTURE));
        auto *surface = create(display, "EGLSurface(20)");
        auto *swapchain = create(display, "EGLSurface(20/0xabc)");
        QVERIFY(surface && swapchain);
        QCOMPARE(wl_display_dispatch_queue_pending(display, surface), 0);
        const auto dispatch = drain ? &wl_display_dispatch_queue_pending : realDispatch;
        QBENCHMARK { dispatch(display, surface); }
        wl_event_queue_destroy(swapchain);
        wl_event_queue_destroy(surface);
    }
};
QTEST_APPLESS_MAIN(EglDrainTest)
#include "test_egldrain.moc"

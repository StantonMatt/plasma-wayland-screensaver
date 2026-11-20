// SPDX-License-Identifier: GPL-3.0-or-later
// No windows or simulated input. ext-idle-notify v2 distinguishes input from
// inhibitors, unlike GetSessionIdleTime and KIdleTime's polling on Wayland.
#include <QCoreApplication>
#include <QDBusConnection>
#include <QDBusInterface>
#include <QDBusReply>
#include <QSocketNotifier>
#include <QTimer>
#include <wayland-client.h>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <unistd.h>

namespace {
// Wire definitions from ext-idle-notify-v1 (version 2):
// https://wayland.app/protocols/ext-idle-notify-v1
// Only the v2
// input notification request is used; normal idle inhibitors cannot mask input.
extern const wl_interface notificationInterface;
const wl_interface *notificationTypes[] = {&notificationInterface, nullptr, &wl_seat_interface};
const wl_message notifierRequests[] = {{"destroy", "", nullptr},
    {"get_idle_notification", "nuo", notificationTypes},
    {"get_input_idle_notification", "2nuo", notificationTypes}};
const wl_interface notifierInterface = {"ext_idle_notifier_v1", 2, 3, notifierRequests, 0, nullptr};
const wl_message notificationRequests[] = {{"destroy", "", nullptr}};
const wl_message notificationEvents[] = {{"idled", "", nullptr}, {"resumed", "", nullptr}};
const wl_interface notificationInterface = {"ext_idle_notification_v1", 2,
    1, notificationRequests, 2, notificationEvents};
wl_proxy *notifier = nullptr;
wl_seat *seat = nullptr;
void emitEvent(const char *event) { std::puts(event); std::fflush(stdout); }
void global(void *, wl_registry *registry, uint32_t name, const char *interface, uint32_t version)
{
    if (!std::strcmp(interface, notifierInterface.name) && version >= 2)
        notifier = static_cast<wl_proxy *>(wl_registry_bind(registry, name, &notifierInterface, 2));
    if (!seat && !std::strcmp(interface, "wl_seat"))
        seat = static_cast<wl_seat *>(wl_registry_bind(registry, name, &wl_seat_interface, 1));
}
void removed(void *, wl_registry *, uint32_t) {}
void idled(void *, wl_proxy *) { emitEvent("IDLE"); }
void resumed(void *, wl_proxy *) { emitEvent("RESUMED"); }
}

int main(int argc, char **argv)
{
    QCoreApplication app(argc, argv);
    if (argc != 2) return 2;
    char *end = nullptr;
    const long timeout = std::strtol(argv[1], &end, 10);
    if (!end || *end || timeout < 1 || timeout > 2147483647) return 2;
    wl_display *display = wl_display_connect(nullptr);
    if (!display) { std::fputs("Cannot connect to Wayland\n", stderr); return 1; }
    auto *registry = wl_display_get_registry(display);
    const wl_registry_listener registryListener = {global, removed};
    wl_registry_add_listener(registry, &registryListener, nullptr);
    if (wl_display_roundtrip(display) < 0 || !notifier || !seat) {
        std::fputs("Need ext_idle_notifier_v1 version 2 and a seat; refusing unguarded real run\n", stderr);
        return 1;
    }
    auto *notification = static_cast<wl_proxy *>(wl_proxy_marshal_flags(notifier, 2,
        &notificationInterface, 2, 0, nullptr, uint32_t(timeout), seat));
    struct Listener { void (*idle)(void *, wl_proxy *); void (*resume)(void *, wl_proxy *); };
    const Listener listener = {idled, resumed};
    wl_proxy_add_listener(notification, reinterpret_cast<void (**)(void)>(const_cast<Listener *>(&listener)), nullptr);
    if (wl_display_roundtrip(display) < 0) return 1;
    QSocketNotifier wayland(wl_display_get_fd(display), QSocketNotifier::Read);
    QObject::connect(&wayland, &QSocketNotifier::activated, &app, [&] {
        if (wl_display_dispatch(display) < 0) app.exit(1);
    });
    // Persistent D-Bus ownership keeps these inhibitors alive until cleanup.
    // Input-idle notifications explicitly ignore these inhibitors.
    QDBusInterface screen("org.freedesktop.ScreenSaver", "/ScreenSaver", "org.freedesktop.ScreenSaver");
    QDBusInterface power("org.freedesktop.PowerManagement", "/org/freedesktop/PowerManagement/Inhibit",
        "org.freedesktop.PowerManagement.Inhibit");
    uint screenCookie = 0, powerCookie = 0;
    bool screenHeld = false, powerHeld = false;
    QSocketNotifier input(STDIN_FILENO, QSocketNotifier::Read);
    QObject::connect(&input, &QSocketNotifier::activated, &app, [&] {
        char buffer[64];
        const auto length = read(STDIN_FILENO, buffer, sizeof(buffer));
        if (length <= 0) { app.quit(); return; }
        if (QByteArray(buffer, length).trimmed() != "inhibit" || screenHeld) { app.exit(1); return; }
        const QDBusReply<uint> a = screen.call("Inhibit", "PVS visual benchmark", "Measuring idle previews");
        if (a.isValid()) { screenCookie = a.value(); screenHeld = true; }
        const QDBusReply<uint> b = power.call("Inhibit", "PVS visual benchmark", "Measuring idle previews");
        if (b.isValid()) { powerCookie = b.value(); powerHeld = true; }
        if (!screenHeld || !powerHeld) {
            std::fputs("Cannot inhibit screen locking and display power management\n", stderr);
            app.exit(1);
        } else emitEvent("INHIBITED");
    });
    QTimer heartbeat;
    QObject::connect(&heartbeat, &QTimer::timeout, &app, [] { emitEvent("HEARTBEAT"); });
    heartbeat.start(500);
    emitEvent("READY");
    const int result = app.exec();
    if (screenHeld) screen.call("UnInhibit", screenCookie);
    if (powerHeld) power.call("UnInhibit", powerCookie);
    wl_proxy_marshal_flags(notification, 0, nullptr, 2, WL_MARSHAL_FLAG_DESTROY);
    wl_proxy_marshal_flags(notifier, 0, nullptr, 2, WL_MARSHAL_FLAG_DESTROY);
    wl_seat_destroy(seat);
    wl_registry_destroy(registry);
    wl_display_disconnect(display);
    return result;
}

// SPDX-License-Identifier: GPL-3.0-or-later
#include "livingvisualitem.h"
#include "visualdepthregistry.h"
#include "visualworldcoordinator.h"
#include "presentationclock.h"
#include <QTest>
#include <QGuiApplication>
#include <QSGNode>
#include <QElapsedTimer>
#include <cmath>
#include <cstdlib>
#include <cstring>
#include <future>
namespace
{
thread_local bool countAllocations = false;
thread_local size_t allocations = 0;
class TestItem final : public LivingVisualItem
{
public:
    QSGNode* synchronize(QSGNode* old) { return updatePaintNode(old, nullptr); }
protected:
    QSGNode* buildDepthNode(QSGNode* old, const VisualSampleLease& lease, const vd_sample_info& info,
        const vd_endpoint& lo, const vd_endpoint& hi) override
    {
        // Geometry is tested without a GPU/window; textures are covered by the
        // private real-KWin capture suite.
        auto geometryOnly = hi;
        geometryOnly.texture_bytes = nullptr;
        return LivingVisualItem::buildDepthNode(old, lease, info, lo, geometryOnly);
    }
};
}
// Count the actual C allocator too: Rust's boxed ABI leases and Qt geometry
// storage bypass C++ operator new. This project targets glibc/Linux/KWin.
extern "C" void* __libc_malloc(size_t) noexcept;
extern "C" void* __libc_calloc(size_t, size_t) noexcept;
extern "C" void* __libc_realloc(void*, size_t) noexcept;
extern "C" void* malloc(size_t size) noexcept
{
    if (countAllocations) ++allocations;
    return __libc_malloc(size);
}
extern "C" void* calloc(size_t n, size_t size) noexcept
{
    if (countAllocations) ++allocations;
    return __libc_calloc(n, size);
}
extern "C" void* realloc(void* p, size_t size) noexcept
{
    if (countAllocations) ++allocations;
    return __libc_realloc(p, size);
}
class DepthWorldTest final : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void reducedMotionWake()
    {
        const QVector<VisualOutput> outputs = {
            { QStringLiteral("Virtual-0"), { 0, 0, 1920, 1080 }, 1., 60., true, true }
        };
        VisualEnvironment environment;
        for (const auto& key : VisualDepthRegistry::keys()) {
            const auto id = VisualDepthRegistry::id(key);
            const vd_config config { .visual = id, .palette = 0, .seed = 7,
                .speed = 1., .density = 50., .scale = 1., .trails = .35,
                .reduced_motion = 1, .monitor_mode = 0 };
            const std::vector<vd_output> arenas = { { 1, { 0, 0, 1920, 1080 }, 1., 60., 1, 0 } };
            VisualWorld world(config, {}, arenas, environment.colours, {}, 1, 50'000'000);
            QVERIFY(world.valid());
            if (key == QStringLiteral("ribbons")) {
                QCOMPARE(world.nextWake(0), 600'000'000'000LL);
                QCOMPARE(world.nextWake(600'000'000'000LL), 600'083'333'333LL);
                QCOMPARE(world.nextWake(608'000'000'000LL), 1'200'000'000'000LL);
            } else if (key == QStringLiteral("aurora")) {
                // Aurora's accepted still sky renews at a seeded interval.
                QVERIFY(world.nextWake(0) >= 600'000'000'000LL);
                QVERIFY(world.nextWake(0) <= 605'000'000'000LL);
            } else if (key == QStringLiteral("orbs")) {
                // Orbs already had this wake on 0.27.0; preserve its schedule.
                QCOMPARE(world.nextWake(0), 360'000'000'000LL);
                QCOMPARE(world.nextWake(604'000'000'000LL), 728'000'000'000LL);
            } else {
                QCOMPARE(world.nextWake(0), -1LL);
                QCOMPARE(world.nextWake(604'000'000'000LL), -1LL);
            }
        }
        // Backdate this private coordinator to just before its ten-minute wake.
        // The real GUI single-shot must publish and continue the dissolve.
        const vd_config config { .visual = VisualDepthRegistry::id(QStringLiteral("ribbons")), .palette = 0, .seed = 7,
            .speed = 1., .density = 50., .scale = 1., .trails = .35,
            .reduced_motion = 1, .monitor_mode = 0 };
        VisualWorldCoordinator coordinator;
        const auto epoch = VisualWorldCoordinator::monotonicNow() - 599'800'000'000LL;
        QVERIFY(coordinator.configure(config, {}, outputs, VisualMonitorMode::Independent,
            VisualMirroredPolicy::SharedCanonical, environment, epoch));
        QVERIFY(!coordinator.sampleFor(QStringLiteral("Virtual-0")));
        QVERIFY(QTest::qWaitFor([&] { return bool(coordinator.sampleFor(QStringLiteral("Virtual-0"))); }, 1500));
        const auto timeline = [&] {
            vd_sample_info info {};
            const auto sample = coordinator.sampleFor(QStringLiteral("Virtual-0"));
            return sample && visuals_depth_sample_info(sample.get(), &info) == 0 ? info.view.timeline_ns : -1;
        };
        const auto first = timeline();
        QVERIFY(first >= 600'000'000'000LL);
        QVERIFY(QTest::qWaitFor([&] {
            return timeline() > first;
        }, 1000));
        coordinator.stop();
        QCOMPARE(coordinator.worldCount(), 0);
    }
    void seedsAndReleasedSlots()
    {
        const auto visuals = VisualDepthRegistry::keys();
        QVERIFY(!visuals.isEmpty());
        for (const auto& visual : visuals) {
            const auto id = VisualDepthRegistry::id(visual);
            QVERIFY(id);
            QCOMPARE(VisualDepthRegistry::available(visual), bool(visuals_depth_available(id)));
            const vd_parameter* parameters = nullptr;
            size_t count = 0;
            QCOMPARE(visuals_depth_schema(id, &parameters, &count), 0);
            QCOMPARE(size_t(VisualDepthRegistry::schema(visual).size()), count);
        }
        QCOMPARE(VisualWorldCoordinator::outputIdentity(QStringLiteral("DP-5")), quint64(0x174b27724a34bbebULL));
        QVERIFY(VisualWorldCoordinator::independentSeed(7, 2, QStringLiteral("DP-5"))
            != VisualWorldCoordinator::independentSeed(7, 2, QStringLiteral("DP-4")));
    }
#ifdef PVS_ENABLE_VISUAL_CAPTURE
    void sharingRecreationAndLeaseThreads()
    {
        const vd_config config { 65535, 0, 7, 1., 50., 1., .35, 0, 0 };
        QVector<VisualOutput> outputs = { { QStringLiteral("HDMI-A-5"), { 0, 0, 2560, 1440 }, 1., 60., false, true },
            { QStringLiteral("DP-5"), { 2560, 0, 3440, 1440 }, 1., 175., true, true },
            { QStringLiteral("DP-4"), { 6000, 195, 1920, 1080 }, 1., 240., false, true } };
        VisualEnvironment environment;
        environment.colours.background = 3;
        environment.clockRects.insert(QStringLiteral("HDMI-A-5"), QRectF(100, 100, 200, 50));
        environment.clockRects.insert(QStringLiteral("DP-5"), QRectF(20, 30, 220, 60));
        environment.clockRects.insert(QStringLiteral("DP-4"), QRectF(40, 50, 240, 70));
        VisualWorldCoordinator coordinator;
        outputs[2].configured = false;
        QVERIFY(!coordinator.configure(config, {}, outputs, VisualMonitorMode::Spanning,
            VisualMirroredPolicy::SharedCanonical, environment, 1000, 1000));
        QCOMPARE(coordinator.worldCount(), 0);
        outputs[2].configured = true;
        for (auto mode : { VisualMonitorMode::Spanning, VisualMonitorMode::Mirrored, VisualMonitorMode::Independent })
        {
            QVERIFY(coordinator.configure(
                config, {}, outputs, mode, VisualMirroredPolicy::SharedCanonical, environment, 1000, 1000));
            QCOMPARE(coordinator.worldCount(), mode == VisualMonitorMode::Independent ? 3 : 1);
            QVERIFY(coordinator.present(QStringLiteral("DP-5"), 1000));
            const auto old = coordinator.sampleFor(QStringLiteral("DP-5"));
            vd_sample_info info {};
            QCOMPARE(visuals_depth_sample_info(old.get(), &info), 0);
            vd_endpoint endpoint {};
            QCOMPARE(visuals_depth_sample_endpoint(old.get(), 0, &endpoint), 0);
            QVERIFY(endpoint.payload_size >= 16);
            quint32 echoedMode = 0, background = 0;
            quint64 obstacles = 0;
            std::memcpy(&echoedMode, endpoint.payload, 4);
            std::memcpy(&background, endpoint.payload + 4, 4);
            std::memcpy(&obstacles, endpoint.payload + 8, 8);
            QCOMPARE(echoedMode, uint32_t(mode));
            QCOMPARE(background, 3U);
            QCOMPARE(obstacles, mode == VisualMonitorMode::Spanning ? 3ULL : 1ULL);
            if (mode == VisualMonitorMode::Mirrored)
            {
                double x = 0, y = 0;
                std::memcpy(&x, endpoint.payload + 16, 8);
                std::memcpy(&y, endpoint.payload + 24, 8);
                QCOMPARE(x, 20.);
                QCOMPARE(y, 30.);
            }

            // Presenting DP-5 never replaces another output's retained lease.
            const auto other = coordinator.sampleFor(QStringLiteral("DP-4"));
            if (other)
            {
                vd_sample_info otherInfo {};
                QCOMPARE(visuals_depth_sample_info(other.get(), &otherInfo), 0);
                QVERIFY(otherInfo.generations.layout < coordinator.layoutGeneration());
            }
            QVERIFY(coordinator.present(QStringLiteral("DP-4"), 1000));
            const auto ticks = coordinator.statistics().first().ticks;
            QVERIFY(coordinator.present(QStringLiteral("DP-5"), 1000));
            QCOMPARE(coordinator.statistics().first().ticks, ticks);
            const auto generation = coordinator.layoutGeneration();
            outputs[0].viewport.translate(-10, 0);
            QVERIFY(coordinator.configure(
                config, {}, outputs, mode, VisualMirroredPolicy::SharedCanonical, environment, 1000, 1000));
            QVERIFY(coordinator.layoutGeneration() > generation);
            QCOMPARE(coordinator.epoch(), 1000);
            // A render-thread reader keeps immutable endpoints after world retirement.
            auto future = std::async(std::launch::async,
                [old]
                {
                    vd_endpoint endpoint {};
                    return visuals_depth_sample_endpoint(old.get(), 0, &endpoint) == 0 && endpoint.circle_count > 0;
                });
            QVERIFY(future.get());
            QCOMPARE(visuals_depth_sample_info(old.get(), &info), 0);
        }
        QVERIFY(coordinator.configure(
            config, {}, outputs, VisualMonitorMode::Mirrored, VisualMirroredPolicy::PerOutputLocal, environment, 1000, 1000));
        QCOMPARE(coordinator.worldCount(), 3);
        // A present call cannot perform unbounded catch-up.
        QVERIFY(coordinator.present(QStringLiteral("DP-5"), 10'000'001'000));
        QCOMPARE(coordinator.statistics().at(1).ticks, 10ULL);
        QVERIFY(coordinator.inject(1, 1'000'000'000, { 200, 300 }));
        QVERIFY(coordinator.seek(3'000'000'000));
        const auto hashes = coordinator.stateHashes();
        QVERIFY(coordinator.seek(0));
        QVERIFY(coordinator.seek(3'000'000'000));
        QCOMPARE(coordinator.stateHashes(), hashes);
        coordinator.stop();
        QCOMPARE(coordinator.worldCount(), 0);
    }
    void recreationStartsNowAndRetainsLastFrame()
    {
        const vd_config config { 65535, 0, 7, 1., 50., 1., .35, 0, 0 };
        const QString connector = QStringLiteral("DP-5");
        QVector<VisualOutput> outputs = { { connector, { 0, 0, 3440, 1440 }, 1., 60., true, true } };
        VisualWorldCoordinator coordinator;
        TestItem item;
        QVERIFY(coordinator.configure(config, {}, outputs, VisualMonitorMode::Spanning,
            VisualMirroredPolicy::SharedCanonical, {}, 1000));
        coordinator.attach(connector, &item);
        QVERIFY(coordinator.present(connector, 1000));
        auto* node = item.synchronize(nullptr);
        QVERIFY(node);
        const auto old = coordinator.sampleFor(connector);
        vd_sample_info original {};
        QCOMPARE(visuals_depth_sample_info(old.get(), &original), 0);
        constexpr qint64 now = 600'000'001'000;
        outputs[0].viewport.translate(-10, 0);
        QVERIFY(coordinator.configure(config, {}, outputs, VisualMonitorMode::Spanning,
            VisualMirroredPolicy::SharedCanonical, {}, 1000, now));
        QCOMPARE(coordinator.sampleFor(connector), old);
        QCOMPARE(item.synchronize(node), node); // no blank synchronization during replacement
        // A late callback publishes the replacement's latest endpoints, with
        // time clamped to their data, then catches up on the next present.
        QVERIFY(coordinator.present(connector, now + 200'000'000));
        vd_sample_info catchingUp {};
        QCOMPARE(visuals_depth_sample_info(coordinator.sampleFor(connector).get(), &catchingUp), 0);
        QCOMPARE(catchingUp.history_miss, 1U);
        QVERIFY(catchingUp.view.timeline_ns < 200'000'000);
        node = item.synchronize(node);
        QVERIFY(node);
        QVERIFY(coordinator.present(connector, now + 200'000'000));
        const auto fresh = coordinator.sampleFor(connector);
        QVERIFY(fresh);
        QVERIFY(fresh != old);
        vd_sample_info info {};
        QCOMPARE(visuals_depth_sample_info(fresh.get(), &info), 0);
        QCOMPARE(info.view.timeline_ns, 200'000'000);
        QVERIFY(info.generations.layout > original.generations.layout);
        QVERIFY(coordinator.statistics().first().ticks <= 13);
        node = item.synchronize(node);
        QVERIFY(node);
        delete node;
        // At the recreation time itself, normal startup needs just one tick.
        QVERIFY(coordinator.configure(config, {}, outputs, VisualMonitorMode::Spanning,
            VisualMirroredPolicy::SharedCanonical, {}, 1000, now));
        QVERIFY(coordinator.present(connector, now));
        QCOMPARE(coordinator.statistics().first().ticks, 1ULL);
    }
    void frameCapsConverge_data()
    {
        QTest::addColumn<int>("cap");
        QTest::addColumn<int>("refresh");
        QTest::addColumn<int>("tickRate");
        // All Configuration::setFrameRate options, including Auto.
        for (int cap : { 0, 15, 24, 30, 45, 60, 75, 90, 100, 120, 144, 165, 175, 200, 240 })
            for (int refresh : { 60, 100, 175, 240 })
                for (int tickRate : { 20, 30, 60, 120 })
                    QTest::addRow("cap%d-refresh%d-tick%d", cap, refresh, tickRate) << cap << refresh << tickRate;
    }
    void frameCapsConverge()
    {
        QFETCH(int, cap);
        QFETCH(int, refresh);
        QFETCH(int, tickRate);
        const int divisor = cap > 0 ? int(std::ceil(double(refresh) / cap)) : 1;
        const long double period = 1'000'000'000.L * divisor / refresh;
        VisualEnvironment environment;
        environment.presentationLeadNs = qint64(std::ceil(period));
        const vd_config config { 65535, 0, 7, 1., 50., 1., .35, 0, 0 };
        const QString connector = QStringLiteral("DP-5");
        VisualWorldCoordinator coordinator;
        QVERIFY(coordinator.configure(config, {}, { { connector, { 0, 0, 1920, 1080 }, 1., double(refresh), true, true } },
            VisualMonitorMode::Independent, VisualMirroredPolicy::SharedCanonical, environment, 0));
        // Fixture physics are 60 Hz: scale only its timeline to reproduce each
        // world's tick demand while keeping the real presentation-period budget.
        // At 100 Hz / cap 15 / world 120 Hz this requests 37 + floor(8.4*i)
        // ticks, reproducing the permanent starvation with the old budget of 8.
        const auto budget = quint64(std::min(32.L, std::ceil(120.L * environment.presentationLeadNs / 1e9L) + 4));
        int firstCurrent = -1;
        int currentTail = 0;
        qint64 priorTimeline = -1;
        for (int i = 0; i < 32; ++i)
        {
            const auto ns = qint64(std::llround((300'000'000.L + i * period) * tickRate / 60));
            const auto before = coordinator.statistics().first().ticks;
            const bool published = coordinator.present(connector, ns);
            QVERIFY(coordinator.statistics().first().ticks - before <= budget);
            if (!published)
                continue;
            vd_sample_info info {};
            QCOMPARE(visuals_depth_sample_info(coordinator.sampleFor(connector).get(), &info), 0);
            QVERIFY(info.view.timeline_ns >= priorTimeline);
            priorTimeline = info.view.timeline_ns;
            if (!info.history_miss && firstCurrent < 0)
                firstCurrent = i;
            if (i >= 16)
            {
                QCOMPARE(info.history_miss, 0U);
                QCOMPARE(info.view.timeline_ns, ns);
                ++currentTail;
            }
        }
        QVERIFY(firstCurrent >= 0 && firstCurrent < 16);
        QCOMPARE(currentTail, 16);
    }
    void cappedCatchUpTiming()
    {
        const vd_config config { 65535, 0, 7, 1., 50., 1., .35, 0, 0 };
        const QString connector = QStringLiteral("DP-5");
        VisualEnvironment environment;
        environment.presentationLeadNs = 1'000'000'000;
        VisualWorldCoordinator coordinator;
        QVERIFY(coordinator.configure(config, {}, { { connector, { 0, 0, 3440, 1440 }, 1., 60., true, true } },
            VisualMonitorMode::Independent, VisualMirroredPolicy::SharedCanonical, environment, 0));
        qint64 slowestPresent = 0;
        for (int i = 0; i < 128; ++i)
        {
            const auto before = coordinator.statistics().first().ticks;
            QElapsedTimer timer;
            timer.start();
            const bool published = coordinator.present(connector, 10'000'000'000LL + i * 1'000'000'000LL);
            slowestPresent = std::max(slowestPresent, timer.nsecsElapsed());
            QVERIFY(published);
            QCOMPARE(coordinator.statistics().first().ticks - before, 32ULL);
            vd_sample_info info {};
            QCOMPARE(visuals_depth_sample_info(coordinator.sampleFor(connector).get(), &info), 0);
            QCOMPARE(info.history_miss, 1U);
            QCOMPARE(info.lower_tick, info.upper_tick);
            QCOMPARE(info.upper_tick, coordinator.statistics().first().ticks);
        }
        const auto stats = coordinator.statistics().first();
        QVERIFY(stats.p99_ns <= 500'000);
        QVERIFY(stats.max_ns <= 2'000'000);
        qInfo("fixture tick+export p99=%llu ns max=%llu ns; 32-tick bound=%llu ns; slowest present=%lld ns",
            qulonglong(stats.p99_ns), qulonglong(stats.max_ns), qulonglong(32 * stats.max_ns), slowestPresent);
    }
    void warmedPresentationDoesNotAllocate()
    {
        const vd_config config { 65535, 0, 7, 1., 50., 1., .35, 0, 0 };
        const QString connector = QStringLiteral("DP-5");
        QVector<VisualOutput> outputs = { { connector, { 0, 0, 3440, 1440 }, 1., 60., true, true } };
        VisualWorldCoordinator coordinator;
        TestItem item;
        QVERIFY(coordinator.configure(config, {}, outputs, VisualMonitorMode::Spanning,
            VisualMirroredPolicy::SharedCanonical, {}, 1000));
        coordinator.attach(connector, &item);
        allocations = 0;
        countAllocations = true;
        const bool first = coordinator.present(connector, 1000);
        countAllocations = false;
        QVERIFY(first);
        QVERIFY(allocations > 0); // verify that the probe sees cold ABI/storage allocations
        QSGNode* node = nullptr;
        for (int i = 0; i < 10; ++i)
        {
            QVERIFY(coordinator.present(connector, 1000 + i * 1000));
            node = item.synchronize(node);
        }
        // Keep one reader pinned: the pool must never mutate its handle or view.
        const auto pinned = coordinator.sampleFor(connector);
        vd_sample_info original {};
        QCOMPARE(visuals_depth_sample_info(pinned.get(), &original), 0);
        for (int i = 0; i < 10; ++i)
        {
            QVERIFY(coordinator.present(connector, 11000 + i * 1000));
            node = item.synchronize(node);
        }
        const auto ticks = coordinator.statistics().first().ticks;
        const auto measuredBefore = PresentationTelemetry::coordinatorTotalNs;
        allocations = 0;
        countAllocations = true;
        bool ready = true;
        for (int i = 0; i < 100; ++i)
        {
            ready &= coordinator.present(connector, 21000 + i * 1000);
            node = item.synchronize(node);
        }
        countAllocations = false;
        QCOMPARE(allocations, size_t(0));
        QVERIFY(ready);
        QVERIFY(PresentationTelemetry::coordinatorTotalNs > measuredBefore);
        QCOMPARE(coordinator.statistics().first().ticks, ticks);
        vd_sample_info retained {};
        QCOMPARE(visuals_depth_sample_info(pinned.get(), &retained), 0);
        QCOMPARE(retained.view.timeline_ns, original.view.timeline_ns);
        QCOMPARE(retained.alpha, original.alpha);
        coordinator.stop();
        vd_endpoint endpoint {};
        QCOMPARE(visuals_depth_sample_endpoint(pinned.get(), 0, &endpoint), 0);
        QVERIFY(endpoint.circle_count > 0);
        delete node;
    }
#endif
};
int main(int argc, char** argv)
{
    qputenv("QT_QPA_PLATFORM", "offscreen");
    qputenv("QT_QUICK_BACKEND", "software");
    qunsetenv("DISPLAY");
    qunsetenv("WAYLAND_DISPLAY");
    qputenv("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent-depth-world-test-bus");
    qputenv("DBUS_SYSTEM_BUS_ADDRESS", "unix:path=/nonexistent-depth-world-system-bus");
    QGuiApplication app(argc, argv);
    DepthWorldTest test;
    return QTest::qExec(&test, argc, argv);
}
#include "test_depthworld.moc"

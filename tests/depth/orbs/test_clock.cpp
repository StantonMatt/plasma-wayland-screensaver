// SPDX-License-Identifier: GPL-3.0-or-later
#include "visuals_depth_orbs.h"
#include "visualworldcoordinator.h"
#include <QLineF>
#include <QTest>

class OrbsClockTest final : public QObject {
    Q_OBJECT
private Q_SLOTS:
    void movingClockInWorldSpace()
    {
        const QVector<VisualOutput> outputs { { QStringLiteral("left"), { -2560, 0, 2560, 1440 }, 1., 100., false, true },
            { QStringLiteral("centre"), { 0, 0, 3440, 1440 }, 1., 175., true, true },
            { QStringLiteral("right"), { 3440, 195, 1920, 1080 }, 1., 240., false, true } };
        VisualEnvironment env;
        for (const auto& o : outputs) {
            env.clockVelocity.insert(o.connector, QPointF(31., 23.));
            env.clockRects.insert(o.connector, QRectF(200., 140., 180., 72.));
        }
        vd_config config { 8, 0, 7, 3., 50., 1., 0.35, 0, 0 };
        for (const auto mode :
            { VisualMonitorMode::Spanning, VisualMonitorMode::Mirrored, VisualMonitorMode::Independent }) {
            VisualWorldCoordinator coordinator;
            QVERIFY(coordinator.configure(config, {}, outputs, mode, VisualMirroredPolicy::SharedCanonical, env, 0, 0));
            QCOMPARE(coordinator.worldCount(), mode == VisualMonitorMode::Independent ? 3 : 1);
            const auto read = [&](const QString& id, QPointF* p) {
                const auto sample = coordinator.sampleFor(id);
                double xy[2];
                if (!sample || !visuals_orbs_clock(sample.get(), xy))
                    return false;
                *p = { xy[0], xy[1] };
                return true;
            };
            for (const auto& o : outputs)
                QVERIFY(coordinator.present(o.connector, 0));
            QPointF before;
            QVERIFY(read(QStringLiteral("centre"), &before));
            for (int tick = 1; tick <= 30; ++tick)
                for (const auto& o : outputs)
                    QVERIFY(coordinator.present(o.connector, tick * 1000000000LL / 30));
            QPointF after;
            QVERIFY(read(QStringLiteral("centre"), &after));
            QVERIFY(QLineF(after - before, QPointF(31, 23)).length() < 1e-7);
            for (const auto& o : outputs) {
                QPointF pos;
                if (mode == VisualMonitorMode::Mirrored && o.connector != QStringLiteral("centre")) {
                    QVERIFY(read(o.connector, &pos));
                    QVERIFY(QLineF(pos, after).length() < 1e-7);
                    vd_sample_info info {};
                    QCOMPARE(visuals_depth_sample_info(coordinator.sampleFor(o.connector).get(), &info), 0);
                    QCOMPARE(
                        info.view.uniform_scale, std::max(o.viewport.width() / 3440., o.viewport.height() / 1440.));
                } else {
                    QVERIFY(read(o.connector, &pos));
                    if (mode == VisualMonitorMode::Spanning)
                        QVERIFY(QLineF(pos + o.viewport.topLeft(), after).length() < 1e-6);
                    else
                        QCOMPARE(pos, after);
                }
            }
        }
    }
    void oledWakeSleepsBetweenDissolves()
    {
        vd_config c { 8, 0, 4, 1., 50., 1., 0.35, 1, 0 };
        vd_output output { 1, { 0, 0, 800, 450 }, 1., 60., 1, 0 };
        VisualWorld world(c, {}, { output }, {}, {}, 1, 33000000);
        QVERIFY(world.valid());
        QCOMPARE(world.nextWake(0), 360000000000LL);
        QVERIFY(world.nextWake(361000000000LL) - 361000000000LL < 100000000);
        QCOMPARE(world.nextWake(368000000000LL), 728000000000LL);
    }
};
QTEST_GUILESS_MAIN(OrbsClockTest)
#include "test_clock.moc"

// SPDX-License-Identifier: GPL-3.0-or-later
#include "configuration.h"
#include "livingvisualitem.h"
#include "overlaymanager.h"
#include "presentationclock.h"
#include "visualworldcoordinator.h"
#include <QDir>
#include <QElapsedTimer>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QPainter>
#include <QQuickView>
#include <QSGRendererInterface>
#include <QScreen>
#include <QScopeGuard>
#include <QTemporaryDir>
#include <QTest>
#include <QProcess>
#include <QStandardPaths>
#include "visualdepthregistry.h"
using namespace Qt::StringLiterals;
class DepthCaptureTest final : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void registeredSceneCatalogs()
    {
        QDir catalogs(QStringLiteral(DEPTH_SCENES_DIR));
        for (const auto& visual : VisualDepthRegistry::keys()) {
            const auto path = catalogs.filePath(visual + QStringLiteral("/round.json"));
            if (!VisualDepthRegistry::available(visual) && !QFileInfo::exists(path))
                continue;
            QFile file(path);
            QVERIFY2(file.open(QIODevice::ReadOnly), qPrintable(path));
            const auto catalog = QJsonDocument::fromJson(file.readAll()).object();
            QVERIFY2(catalog["eventIds"_L1].isArray(), qPrintable(visual));
            QVERIFY2(!catalog["scenes"_L1].toObject().isEmpty(), qPrintable(visual));
        }
        for (const auto& visual : catalogs.entryList(QDir::Dirs | QDir::NoDotAndDotDot)) {
            if (visual != QStringLiteral("fixture") && QFileInfo::exists(catalogs.filePath(visual + "/round.json"_L1)))
                QVERIFY2(VisualDepthRegistry::id(visual), qPrintable(visual));
        }
    }
    void productScene();
    void fixture()
    {
        if (!qEnvironmentVariableIsSet("VISUALS_REQUIRE_GPU"))
            QSKIP("Only the private virtual-KWin runner may run this fixture");
        const auto directory = qEnvironmentVariable("VISUALS_CAPTURE_DIR");
        QVERIFY(!directory.isEmpty());
        const QDir privateRoot(qEnvironmentVariable("PVS_PRIVATE_HARNESS_ROOT"));
        QVERIFY(!qEnvironmentVariable("PVS_PRIVATE_HARNESS_ROOT").isEmpty());
        QCOMPARE(QFileInfo(qEnvironmentVariable("XDG_RUNTIME_DIR")).absolutePath(), privateRoot.absolutePath());
        QVERIFY(qEnvironmentVariable("WAYLAND_DISPLAY").startsWith(QStringLiteral("depth-private-")));
        QVERIFY(qEnvironmentVariable("DBUS_SESSION_BUS_ADDRESS").contains(privateRoot.absolutePath()));
        QTemporaryDir configDir;
        Configuration config(configDir.filePath(QStringLiteral("settingsrc")));
        config.setVisualModule(QStringLiteral("none"));
        config.setShowClock(false);
        config.setFrameRate(60);
        const auto script = qEnvironmentVariable("PVS_VISUAL_CAPTURE_SCRIPT");
        QVERIFY(QFileInfo(script).isAbsolute());
        QFile input(script);
        QVERIFY(input.open(QIODevice::ReadOnly));
        const auto request = QJsonDocument::fromJson(input.readAll()).object();
        if (request["visual"_L1].toString() != QStringLiteral("fixture"))
            QSKIP("The product slot drives visual scenes");
        QCOMPARE(QGuiApplication::screens().size(), 3);
        QCOMPARE(request.value(QStringLiteral("formatVersion")).toInt(), 1);
        const auto mode = request.value(QStringLiteral("monitorMode")).toString();
        const QStringList modes
            = { QStringLiteral("seamless"), QStringLiteral("synchronized"), QStringLiteral("independent") };
        QVERIFY(modes.contains(mode));
        config.setMonitorBehavior(mode);
        bool seedOk = false;
        const auto seed = request.value(QStringLiteral("seed")).toString().toULongLong(&seedOk);
        QVERIFY(seedOk);
        QList<qint64> times;
        for (const auto& time : request.value(QStringLiteral("captureTimesNs")).toArray())
        {
            bool ok = false;
            const auto ns = time.toString().toLongLong(&ok);
            QVERIFY(ok && ns >= 0);
            times.append(ns);
        }
        QVERIFY(!times.isEmpty());
        OverlayManager manager(&config);
        manager.m_captureFixture = true;
        manager.m_captureManualTime = true;
        manager.m_captureSeed = seed;
        manager.m_captureSeedSet = true;
        const auto activationStart = VisualWorldCoordinator::monotonicNow();
        QVERIFY(manager.show());
        QVector<std::shared_ptr<std::atomic<qint64>>> firstFrameTimes;
        for (auto* item : manager.m_depthItems)
        {
            const auto timestamp = std::make_shared<std::atomic<qint64>>(0);
            firstFrameTimes.append(timestamp);
            // The base acknowledgement is connected first. Record the actual
            // render-thread callback, independent of GUI polling delay.
            connect(
                item->window(), &QQuickWindow::afterRendering, item->window(),
                [timestamp, item]
                {
                    if (item->renderedSample() > 0)
                    {
                        qint64 empty = 0;
                        timestamp->compare_exchange_strong(empty, VisualWorldCoordinator::monotonicNow());
                    }
                },
                Qt::DirectConnection);
        }
        // Expose is reached only after layer-shell configure. No world is made
        // from the initially requested single-screen geometries.
        QVERIFY(QTest::qWaitFor(
            [&] { return manager.m_mappingViews.isEmpty() && manager.m_depthCoordinator->worldCount() > 0; }, 10000));
        QCOMPARE(manager.m_depthCoordinator->worldCount(), mode == QStringLiteral("independent") ? 3 : 1);
        for (auto* clock : manager.m_presentationClocks)
            clock->setRunning(false);
        for (const auto& value : request.value(QStringLiteral("events")).toArray())
        {
            const auto event = value.toObject();
            bool ok = false;
            const auto ns = event.value(QStringLiteral("timelineNs")).toString().toLongLong(&ok);
            QVERIFY(ok && ns >= 0);
            const auto anchor = event.value(QStringLiteral("anchor")).toArray();
            QCOMPARE(anchor.size(), 2);
            QVERIFY(manager.m_depthCoordinator->inject(uint32_t(event.value(QStringLiteral("id")).toInt()), ns,
                { anchor[0].toDouble(), anchor[1].toDouble() }, event.value(QStringLiteral("strength")).toDouble(1.)));
        }
        QJsonArray frames;
        QImage panorama(7920, 1440, QImage::Format_ARGB32_Premultiplied);
        qint64 firstFrameNs = 0;
        for (qint64 ns : times)
        {
            QVERIFY(manager.m_depthCoordinator->seek(ns));
            for (auto* screen : manager.m_views.keys())
                QVERIFY(manager.m_depthCoordinator->present(screen->name(), manager.m_depthEpochNs + ns));
            for (auto* item : manager.m_depthItems)
            {
                vd_sample_info info {};
                const auto lease = manager.m_depthCoordinator->sampleFor(
                    manager.m_views.key(qobject_cast<QQuickView*>(item->window()))->name());
                QCOMPARE(visuals_depth_sample_info(lease.get(), &info), 0);
                QTRY_COMPARE_WITH_TIMEOUT(item->renderedTimelineNs(), info.view.timeline_ns, 10000);
                QTRY_COMPARE_WITH_TIMEOUT(item->renderedSample(), info.generations.sample + 1, 10000);
                QCOMPARE(item->renderedLayout(), info.generations.layout);
                QCOMPARE(item->renderedDataRevision(), info.generations.data);
            }
            if (!firstFrameNs)
            {
                qint64 last = 0;
                for (const auto& time : firstFrameTimes)
                {
                    QTRY_VERIFY_WITH_TIMEOUT(time->load() > 0, 10000);
                    last = std::max(last, time->load());
                }
                firstFrameNs = last - activationStart;
            }
            panorama.fill(Qt::black);
            QPainter painter(&panorama);
            QJsonArray outputs;
            for (auto* screen : manager.m_views.keys())
            {
                auto* view = manager.m_views.value(screen);
                QCOMPARE(view->rendererInterface()->graphicsApi(), QSGRendererInterface::OpenGL);
                const auto image = view->grabWindow();
                QVERIFY(!image.isNull());
                const auto geometry = view->geometry();
                const QString role = geometry.x() == 0 ? QStringLiteral("HDMI-A-5")
                    : geometry.x() == 2560             ? QStringLiteral("DP-5")
                                                       : QStringLiteral("DP-4");
                const QString path = directory + QStringLiteral("/%1-%2.png").arg(ns).arg(role);
                QVERIFY(image.save(path));
                painter.drawImage(geometry.topLeft(), image);
                vd_sample_info info {};
                QCOMPARE(
                    visuals_depth_sample_info(manager.m_depthCoordinator->sampleFor(screen->name()).get(), &info), 0);
                vd_endpoint endpoint{};QCOMPARE(visuals_depth_sample_endpoint(manager.m_depthCoordinator->sampleFor(screen->name()).get(),1,&endpoint),0);
                if(ns==2'000'000'000)QCOMPARE(endpoint.texture_size,size_t(0));
                if(!endpoint.texture_size)QVERIFY(image.pixelColor(4,4).red()<30); // no stale texture over the background
                outputs.append(QJsonObject { { QStringLiteral("connector"), screen->name() },
                    { QStringLiteral("role"), role }, { QStringLiteral("path"), path },
                    { QStringLiteral("sample"), QString::number(info.generations.sample) },
                    { QStringLiteral("layoutGeneration"), QString::number(info.generations.layout) },
                    { QStringLiteral("topology"), QString::number(info.generations.topology) },
                    { QStringLiteral("dataRevision"), QString::number(info.generations.data) },
                    {QStringLiteral("texturePresent"),endpoint.texture_size>0} });
            }
            painter.end();
            const QString path = directory + QStringLiteral("/%1-world.png").arg(ns);
            QVERIFY(panorama.save(path));
            QJsonArray hashes;
            for (auto hash : manager.m_depthCoordinator->stateHashes())
                hashes.append(QString::number(hash));
            frames.append(QJsonObject { { QStringLiteral("timelineNs"), QString::number(ns) },
                { QStringLiteral("outputs"), outputs }, { QStringLiteral("panorama"), path },
                { QStringLiteral("stateHashes"), hashes } });
        }
        QJsonArray timings;
        for (const auto& stats : manager.m_depthCoordinator->statistics())
            timings.append(QJsonObject { { QStringLiteral("ticks"), QString::number(stats.ticks) },
                { QStringLiteral("tickExportP99Ns"), QString::number(stats.p99_ns) },
                { QStringLiteral("tickExportMaxNs"), QString::number(stats.max_ns) },
                { QStringLiteral("snapshotBytes"), QString::number(stats.snapshot_bytes) } });
        // Recreate with the same asset revision and a changed deterministic seed.
        // A retained QSG node must key uploads by world/layout generation as well
        // as revision; otherwise the new world silently displays the old texture.
        QVERIFY(manager.m_depthCoordinator->seek(0));
        for (auto* screen : manager.m_views.keys())
            QVERIFY(manager.m_depthCoordinator->present(screen->name(), manager.m_depthEpochNs));
        for (auto* item : manager.m_depthItems)
            QTRY_COMPARE_WITH_TIMEOUT(item->renderedDataRevision(), 1ULL, 10000);
        QQuickView* reference = nullptr;
        for (auto* view : manager.m_views)
            if (view->x() == 0)
                reference = view;
        QVERIFY(reference);
        const auto before = reference->grabWindow();
        vd_endpoint oldData{};const auto oldLease=manager.m_depthCoordinator->sampleFor(manager.m_views.key(reference)->name());
        QCOMPARE(visuals_depth_sample_endpoint(oldLease.get(),0,&oldData),0);QVERIFY(oldData.texture_size>=4);const auto oldRed=oldData.texture_bytes[0];
        const auto previousGeneration = manager.m_depthCoordinator->layoutGeneration();
        manager.m_captureSeed = seed + 1;
        manager.m_depthLayoutKey.clear();
        manager.configureDepthWorlds();
        QVERIFY(manager.m_depthCoordinator->layoutGeneration() > previousGeneration);
        QVERIFY(manager.m_depthCoordinator->seek(0));
        for (auto* screen : manager.m_views.keys())
            QVERIFY(manager.m_depthCoordinator->present(screen->name(), manager.m_depthEpochNs));
        for (auto* item : manager.m_depthItems)
            QTRY_COMPARE_WITH_TIMEOUT(item->renderedLayout(), manager.m_depthCoordinator->layoutGeneration(), 10000);
        const auto after = reference->grabWindow();
        vd_endpoint newData{};const auto newLease=manager.m_depthCoordinator->sampleFor(manager.m_views.key(reference)->name());
        QCOMPARE(visuals_depth_sample_endpoint(newLease.get(),0,&newData),0);QVERIFY(newData.texture_size>=4);
        const bool textureChanged=oldRed!=newData.texture_bytes[0];
        QCOMPARE(before.pixelColor(4,4)!=after.pixelColor(4,4),textureChanged);
        QVERIFY(before.save(directory + QStringLiteral("/recreation-before.png")));
        QVERIFY(after.save(directory + QStringLiteral("/recreation-after.png")));
        const auto recreatedGeneration=manager.m_depthCoordinator->layoutGeneration();
        // The composed still must be published even though no presentation
        // timer runs. This stays on the same three private GPU outputs.
        manager.m_captureManualTime=false;config.setReducedMotion(true);
        for(auto *screen:manager.m_views.keys()) {
            const auto lease=manager.m_depthCoordinator->sampleFor(screen->name());QVERIFY(lease);
            vd_sample_info info{};QCOMPARE(visuals_depth_sample_info(lease.get(),&info),0);QCOMPARE(info.view.timeline_ns,0);
            auto *item=manager.m_depthItems.value(screen);QTRY_COMPARE_WITH_TIMEOUT(item->renderedLayout(),info.generations.layout,10000);QCOMPARE(item->renderedTimelineNs(),0);
        }
        for(const auto &stats:manager.m_depthCoordinator->statistics())QCOMPARE(stats.ticks,1ULL);
        const QJsonObject manifest { { QStringLiteral("formatVersion"), 1 },
            { QStringLiteral("visual"), QStringLiteral("fixture") }, { QStringLiteral("monitorMode"), mode },
            { QStringLiteral("seed"), QString::number(seed) },
            { QStringLiteral("settings"),
                QJsonObject { { QStringLiteral("speed"), 100 }, { QStringLiteral("density"), 50 },
                    { QStringLiteral("scale"), 100 }, { QStringLiteral("palette"), QStringLiteral("ocean") },
                    { QStringLiteral("trails"), 35 }, { QStringLiteral("background"), QStringLiteral("midnight") },
                    { QStringLiteral("showClock"), false }, { QStringLiteral("reducedMotion"), false },
                    { QStringLiteral("frameCap"), 60 } } },
            { QStringLiteral("firstFrameNs"), QString::number(firstFrameNs) },
            { QStringLiteral("firstFrameMethod"),
                QStringLiteral("Last-output afterRendering acknowledgement; start before OverlayManager.show") },
            { QStringLiteral("gpuPath"), QStringLiteral("OpenGL RHI") },
            {QStringLiteral("reducedStillPublished"),true}, { QStringLiteral("frames"), frames },
            { QStringLiteral("timings"), timings },
            { QStringLiteral("events"), request.value(QStringLiteral("events")) },
            { QStringLiteral("recreation"),
                QJsonObject { { QStringLiteral("beforeGeneration"), QString::number(previousGeneration) },
                    { QStringLiteral("afterGeneration"),
                        QString::number(recreatedGeneration) },
                    { QStringLiteral("sameAssetRevision"), 1 }, { QStringLiteral("pixelsChanged"), textureChanged } } } };
        QFile file(directory + QStringLiteral("/manifest.json"));
        QVERIFY(file.open(QIODevice::WriteOnly));
        file.write(QJsonDocument(manifest).toJson());
        file.close();
        manager.hide();
        QTRY_COMPARE_WITH_TIMEOUT(manager.m_pendingViewDeletions, 0, 10000);
    }
};
#include "depth/productcapture.inc"
QTEST_MAIN(DepthCaptureTest)
#include "test_depthcapture.moc"

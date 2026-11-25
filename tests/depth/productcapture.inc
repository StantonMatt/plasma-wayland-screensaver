// Included by the private capture test, which has access to OverlayManager's
// configured views. A scene always runs in a freshly isolated application.
#include "matrix/gputimer.h"
#include "visuals_depth_kaleidoscope.h"
#include <array>
#include <cmath>
#include <vector>
void DepthCaptureTest::productScene()
{
    if (!qEnvironmentVariableIsSet("VISUALS_REQUIRE_GPU"))
        QSKIP("Private capture only");
    const QDir dir(qEnvironmentVariable("VISUALS_CAPTURE_DIR"));
    const QString root = qEnvironmentVariable("PVS_PRIVATE_HARNESS_ROOT");
    QVERIFY(!root.isEmpty() && QFileInfo(qEnvironmentVariable("XDG_RUNTIME_DIR")).absolutePath() == root);
    QCOMPARE(qEnvironmentVariable("WAYLAND_DISPLAY"), QStringLiteral("depth-private-capture"));
    QVERIFY(qEnvironmentVariable("DBUS_SESSION_BUS_ADDRESS").contains(root));
    QFile file(qEnvironmentVariable("PVS_VISUAL_CAPTURE_SCRIPT"));
    QVERIFY(file.open(QIODevice::ReadOnly));
    const auto scene = QJsonDocument::fromJson(file.readAll()).object();
    if (scene["visual"_L1].toString() == QStringLiteral("fixture"))
        QSKIP("The fixture slot drives the contract scene");
    const auto options = scene["settings"_L1].toObject();
    const bool legacy = scene["legacy"_L1].toBool();
    const bool fallback = scene["softwareProbe"_L1].toBool();
    if (fallback)
        QQuickWindow::setGraphicsApi(QSGRendererInterface::Software);
    const auto outputs = scene["outputs"_L1].toArray();
    QCOMPARE(outputs.size(), QGuiApplication::screens().size());
    QTemporaryDir temp;
    const auto visual = scene["visual"_L1].toString();
    const auto visualId = VisualDepthRegistry::id(visual);
    QVERIFY2(visualId, qPrintable(visual));
    if (legacy || !visuals_depth_available(visualId))
        VisualDepthRegistry::setAvailabilityForTest(visual, !legacy);
    const auto reset = qScopeGuard([&] { VisualDepthRegistry::setAvailabilityForTest(visual, std::nullopt); });
    Configuration config(temp.filePath(QStringLiteral("scene.rc")));
    config.setVisualModule(visual);
    QCOMPARE(config.visualModule(), visual);
    config.setMonitorBehavior(scene["monitorMode"_L1].toString());
    config.setShowClock(scene["showClock"_L1].toBool(false));
    if (config.showClock()) config.setClockMovement(QStringLiteral("bounce"));
    config.setFrameRate(options["frameCap"_L1].toInt(scene["video"_L1].toObject()["fps"_L1].toInt(60)));
    config.setAnimationSpeed(options["speed"_L1].toInt(config.animationSpeed()));
    config.setAnimationDensity(options["density"_L1].toInt(config.animationDensity()));
    config.setAnimationScale(options["scale"_L1].toInt(config.animationScale()));
    config.setTrailAmount(options["trails"_L1].toInt(config.trailAmount()));
    config.setAnimationPalette(options["palette"_L1].toString(config.animationPalette()));
    config.setBackgroundStyle(options["background"_L1].toString(config.backgroundStyle()));
    config.setReducedMotion(options["reducedMotion"_L1].toBool(config.reducedMotion()));
    for (const auto& entry : config.visualSchema()) {
        const auto key = entry.toMap().value("key"_L1).toString();
        if (options.contains(key))
            config.setVisualParameter(key, options[key].toVariant());
    }
    OverlayManager manager(&config);
    const bool sampleTimeline = !config.reducedMotion() || scene["sampleReducedTimeline"_L1].toBool()
        || scene["sampleReducedTime"_L1].toBool();
    manager.m_captureManualTime = !legacy && sampleTimeline;
    manager.m_captureSeed = scene["seed"_L1].toString().toULongLong();
    manager.m_captureSeedSet = scene.contains("seed"_L1);
    const auto start = VisualWorldCoordinator::monotonicNow();
    QVERIFY(manager.show());
    const auto shown = VisualWorldCoordinator::monotonicNow();
    qint64 fallbackPaintNs = 0;
    QVector<std::shared_ptr<std::atomic<qint64>>> first;
    const auto observeFirstRender = [&](QQuickWindow* view, LivingVisualItem* item) {
        auto timestamp = std::make_shared<std::atomic<qint64>>(0);
        first.append(timestamp);
        connect(
            view, &QQuickWindow::afterRendering, view,
            [item, timestamp] {
                if (!item || item->renderedSample() > 0) {
                    qint64 empty = 0;
                    timestamp->compare_exchange_strong(empty, VisualWorldCoordinator::monotonicNow());
                }
            },
            Qt::DirectConnection);
    };
    if (!fallback) {
        for (auto it = manager.m_views.cbegin(); it != manager.m_views.cend(); ++it)
            observeFirstRender(it.value(), manager.m_depthItems.value(it.key()));
    }
    QVERIFY(QTest::qWaitFor(
        [&] {
            return manager.m_mappingViews.isEmpty()
                && (legacy || manager.m_depthGpuFailed || manager.m_depthCoordinator->worldCount() > 0);
        },
        10000));
    const auto mapped = VisualWorldCoordinator::monotonicNow();
    if (fallback) {
        QVERIFY(QTest::qWaitFor([&] { return manager.m_depthGpuFailed; }, 10000));
        QCOMPARE(manager.m_depthCoordinator->worldCount(), 0);
        for (auto* view : manager.m_views) {
            QCOMPARE(view->rendererInterface()->graphicsApi(), QSGRendererInterface::Software);
            QVERIFY(!view->rootObject()->property("depthActive").toBool());
            auto* layer = view->rootObject()->property("legacyVisual").value<QQuickItem*>();
            QVERIFY(layer);
            QCOMPARE(layer->parentItem()->property("source").toUrl().fileName().toLower(), visual + ".qml"_L1);
            if (layer->metaObject()->indexOfProperty("nativeRenderer") >= 0)
                QVERIFY(layer->property("nativeRenderer").value<QObject*>());
            // A restored legacy layer has no living-world sample to acknowledge.
            observeFirstRender(view, nullptr);
            view->update();
        }
    } else if (!legacy) {
        const bool localWorlds = config.monitorBehavior() == "independent"_L1
            || (config.monitorBehavior() == "synchronized"_L1 && visuals_depth_mirrored_policy(visualId));
        QCOMPARE(manager.m_depthCoordinator->worldCount(), localWorlds ? outputs.size() : 1);
        for (auto* screen : manager.m_views.keys())
            QVERIFY(manager.m_depthCoordinator->present(screen->name(), manager.m_depthEpochNs));
    }
    QVERIFY(QTest::qWaitFor(
        [&] { return std::all_of(first.begin(), first.end(), [](const auto& p) { return p->load() > 0; }); },
        10000));
    if (fallback)
        fallbackPaintNs = VisualWorldCoordinator::monotonicNow() - start;
    qint64 activation = fallbackPaintNs;
    for (auto& stamp : first)
        activation = std::max(activation, stamp->load() - start);
    for (auto* clock : manager.m_presentationClocks) {
        if (config.reducedMotion())
            QVERIFY(!clock->isRunning());
        clock->setRunning(false);
    }
    for (const auto event : scene["events"_L1].toArray()) {
        const auto e = event.toObject();
        const auto anchor = e["anchor"_L1].toArray();
        QVERIFY(manager.m_depthCoordinator->inject(uint32_t(e["id"_L1].toInteger()), e["timelineNs"_L1].toString().toLongLong(),
            QPointF(anchor[0].toDouble(), anchor[1].toDouble()), e["strength"_L1].toDouble(1.)));
    }
    QRect bounds;
    for (auto* view : manager.m_views)
        bounds = bounds.united(view->geometry());
    QImage world(bounds.size(), QImage::Format_RGB32);
    const auto video = scene["video"_L1].toObject();
    const int fps = video["fps"_L1].toInt(60), length = video["frames"_L1].toInt();
    const double cadenceHz = video["cadenceHz"_L1].toDouble(fps);
    QVERIFY(std::isfinite(cadenceHz) && cadenceHz >= 1. && cadenceHz <= 240.);
    QList<qint64> times;
    if (length) {
        for (int i = 0; i < length; ++i)
            times.append(qint64(video["start"_L1].toInt()) * 1000000000
                + (video.contains("cadenceHz"_L1) ? qint64(i * 1000000000. / cadenceHz) : i * 1000000000LL / fps));
    } else
        for (auto time : scene["captureTimesNs"_L1].toArray())
            times.append(time.toString().toLongLong());
    const auto gpuStudy = scene["gpuStudy"_L1].toObject();
    if (!gpuStudy.isEmpty()) {
        times.clear();
        const int frames = gpuStudy["frames"_L1].toInt();
        QVERIFY(frames > 0 && frames <= 36001);
        for (int frame = 0; frame < frames; ++frame)
            times.append(frame * 1000000000LL / 60);
    }
    if (scene["gpuTiming"_L1].toBool()) {
        // Time the normal window render (including offscreen passes) on the GPU. grabWindow() below
        // has its own offscreen target and synchronous readback; keep it out
        // of the timing interval.
        QVERIFY(!times.isEmpty());
        struct GpuOutput {
            QQuickWindow* view;
            std::shared_ptr<std::atomic<int>> swaps;
            std::shared_ptr<CaptureGpuTimer> timer;
            std::array<QMetaObject::Connection, 3> connections;
        };
        std::vector<GpuOutput> measured;
        for (auto* view : manager.m_views) {
            auto swaps = std::make_shared<std::atomic<int>>(0);
            auto gpu = std::make_shared<CaptureGpuTimer>(view->objectName());
            measured.push_back({view, swaps, gpu, {
                connect(view, &QQuickWindow::beforeRendering, view,
                    [gpu, view] { gpu->begin(view); }, Qt::DirectConnection),
                connect(view, &QQuickWindow::afterRendering, view,
                    [gpu, view] { gpu->finish(view); }, Qt::DirectConnection),
                connect(view, &QQuickWindow::frameSwapped, view,
                    [swaps] { ++*swaps; }, Qt::DirectConnection)}});
        }
        QVERIFY(manager.m_depthCoordinator->seek(times.first()));
        qInfo("GPU timing begin");
        int gpuFrames = 0;
        for (qint64 ns : times) {
            std::vector<int> prior;
            for (const auto& output : measured) {
                prior.push_back(output.swaps->load());
                QVERIFY(manager.m_depthCoordinator->present(output.view->screen()->name(), manager.m_depthEpochNs + ns));
                output.view->update();
            }
            QVERIFY(QTest::qWaitFor([&] {
                for (size_t i = 0; i < measured.size(); ++i)
                    if (measured[i].swaps->load() <= prior[i]) return false;
                return true;
            }, 5000));
            if (++gpuFrames == 10)
                for (const auto& output : measured)
                    QVERIFY2(output.timer->samples.load() > 0, "GPU elapsed-time query unavailable");
        }
        qInfo("GPU timing end");
        QJsonArray gpuSamples;
        for (const auto& output : measured) {
            for (const auto& connection : output.connections) disconnect(connection);
            QVERIFY2(output.timer->samples.load() >= times.size() * 9 / 10, "Insufficient valid GPU timer samples");
            gpuSamples.append(QJsonObject{{"window"_L1, output.view->objectName()}, {"samples"_L1, output.timer->samples.load()}});
        }
        if (!gpuStudy.isEmpty()) {
            auto manifest = scene;
            manifest["gpuSamples"_L1] = gpuSamples;
            QFile report(dir.filePath(QStringLiteral("manifest.json")));
            QVERIFY(report.open(QIODevice::WriteOnly));
            report.write(QJsonDocument(manifest).toJson());
            report.close();
            manager.hide();
            QTRY_COMPARE_WITH_TIMEOUT(manager.m_pendingViewDeletions, 0, 10000);
            return;
        }
    }
    const auto movieName = video["name"_L1].toString(QStringLiteral("world"));
    const bool seam = video["seam"_L1].toBool();
    const bool halfScale = seam || video["halfScale"_L1].toBool();
    const auto movieSize = halfScale ? bounds.size() / 2 : bounds.size();
    QRect crop;
    if (seam) {
        const auto rect = video["crop"_L1].toArray();
        QCOMPARE(rect.size(), 4);
        crop = QRect(rect[0].toInt(), rect[1].toInt(), rect[2].toInt(), rect[3].toInt());
        QVERIFY(crop.isValid() && world.rect().contains(crop));
    }
    const auto fallbackPixels = scene["fallbackPixels"_L1].toObject();
    const bool encode = length && !QStandardPaths::findExecutable(QStringLiteral("ffmpeg")).isEmpty();
    QProcess movie, detail;
    const auto openMovie = [&](QProcess& encoder, QString name, QSize size) {
        encoder.setStandardErrorFile(dir.filePath(name + QStringLiteral(".log")));
        encoder.start(QStringLiteral("ffmpeg"),
            { QStringLiteral("-loglevel"), QStringLiteral("error"), QStringLiteral("-f"),
                QStringLiteral("rawvideo"), QStringLiteral("-pix_fmt"), QStringLiteral("bgra"),
                QStringLiteral("-s"), QStringLiteral("%1x%2").arg(size.width()).arg(size.height()),
                QStringLiteral("-framerate"), QString::number(fps), QStringLiteral("-i"), QStringLiteral("-"),
                QStringLiteral("-threads"), QStringLiteral("4"), QStringLiteral("-c:v"),
                QStringLiteral("libx264rgb"), QStringLiteral("-preset"), QStringLiteral("ultrafast"),
                QStringLiteral("-crf"), QStringLiteral("0"), QStringLiteral("-pix_fmt"), QStringLiteral("bgr24"),
                dir.filePath(name + QStringLiteral(".mp4")) });
        return encoder.waitForStarted();
    };
    if (encode) {
        QVERIFY(openMovie(movie, movieName, movieSize));
        if (seam)
            QVERIFY(openMovie(detail, QStringLiteral("seam"), crop.size()));
    }
    const auto stream = [&](QProcess& encoder, const QImage& image) {
        if (encoder.write(reinterpret_cast<const char*>(image.constBits()), image.sizeInBytes())
            != image.sizeInBytes())
            return false;
        while (encoder.bytesToWrite() > image.sizeInBytes())
            if (!encoder.waitForBytesWritten(30000))
                return false;
        return true;
    };
    const auto offsets = scene["presentationOffsetsNs"_L1].toObject();
    const auto timeFor = [&](QScreen* screen, qint64 ns) {
        for (const auto& value : outputs) {
            const auto output = value.toObject();
            if (output["x"_L1].toInt() == manager.m_views.value(screen)->x())
                return ns + offsets.value(output["role"_L1].toString()).toInteger();
        }
        return ns;
    };
    // A lossless movie samples its physical outputs at movie FPS. Optionally
    // render every intermediate presentation of one faster output and encode
    // its most recent frame, as an asynchronous monitor would display it.
    const auto cadence = video["interpolation"_L1].toObject();
    const int sampleHz = cadence["hz"_L1].toInt();
    const int pacedX = cadence["worldX"_L1].toInt();
    if (!cadence.isEmpty()) {
        QVERIFY(sampleHz >= fps && sampleHz <= 240 && length && !config.reducedMotion());
        QVERIFY(std::any_of(outputs.begin(), outputs.end(), [=](const auto& o) {
            return o.toObject()["x"_L1].toInt() == pacedX;
        }));
    }
    qint64 lastPacedFrame = -1;
    QJsonArray frames;
    QMap<QString, qint64> kaleidoscopeSwapStarts;
    QMap<QString, double> kaleidoscopeQueueDelays;
    QMap<QString, qint64> kaleidoscopeRefillStarts;
    int index = 0;
    int fallbackBrightPixels = 0;
    for (qint64 ns : times) {
        if (!legacy && !fallback && sampleTimeline) {
            if (index == 0 || !length)
                QVERIFY(manager.m_depthCoordinator->seek(ns));
            for (auto* screen : manager.m_views.keys())
                if (!sampleHz || manager.m_views.value(screen)->x() != pacedX)
                    QVERIFY(manager.m_depthCoordinator->present(screen->name(), manager.m_depthEpochNs + timeFor(screen, ns)));
        }
        if (visual == QStringLiteral("kaleidoscope") && !legacy && !fallback && sampleTimeline) {
            for (auto* screen : manager.m_views.keys()) {
                const auto lease = manager.m_depthCoordinator->sampleFor(screen->name());
                kaleidoscope_lens lens {};
                QVERIFY(visuals_depth_kaleidoscope_sample(lease.get(), &lens, nullptr, 0) > 0);
                const double age = lens.flare[3];
                // Ignore the no-swap sentinel and recipe-boundary endpoint.
                // Ages below one second retain sub-microsecond precision.
                if (age > 0.05 && age < 9.99
                    && (!kaleidoscopeSwapStarts.contains(screen->name()) || age < 1.)) {
                    const auto estimate = timeFor(screen, ns) - qRound64(age * 1e9);
                    const auto tick = qRound64(estimate * (120. / 1e9));
                    if (kaleidoscopeSwapStarts.value(screen->name(), -1) != tick * 1000000000LL / 120)
                        kaleidoscopeRefillStarts.remove(screen->name());
                    kaleidoscopeSwapStarts[screen->name()] = tick * 1000000000LL / 120;
                    kaleidoscopeQueueDelays[screen->name()] = lens.mote[3];
                }
                const double phase = -1. - lens.motion[2];
                if (phase >= 4. && phase < 7.) {
                    const auto estimate = timeFor(screen, ns) - qRound64((phase - 4.) * 1e9);
                    const auto tick = qRound64(estimate * (120. / 1e9));
                    kaleidoscopeRefillStarts[screen->name()] = tick * 1000000000LL / 120;
                }
            }
        }
        world.fill(Qt::black);
        QPainter painter(&world);
        QJsonArray paths, clocks;
        for (auto* screen : manager.m_views.keys()) {
            auto* view = manager.m_views.value(screen);
            auto* item = manager.m_depthItems.value(screen);
            if (item && !fallback) {
                // grabWindow synchronizes and renders at the already bound time.
                QCOMPARE(view->rendererInterface()->graphicsApi(), QSGRendererInterface::OpenGL);
                QVERIFY(!manager.m_depthGpuFailed);
            }
            qint64 sampledNs = timeFor(screen, ns);
            if (sampleHz && view->x() == pacedX) {
                QVERIFY(item);
                const qint64 sampleIndex = qint64(index) * sampleHz / fps;
                while (lastPacedFrame < sampleIndex) {
                    sampledNs = times.front() + (++lastPacedFrame) * 1000000000LL / sampleHz;
                    QVERIFY(manager.m_depthCoordinator->present(screen->name(), manager.m_depthEpochNs + sampledNs));
                    if (lastPacedFrame != sampleIndex) {
                        QVERIFY(!view->grabWindow().isNull());
                        QCOMPARE(item->renderedTimelineNs(), sampledNs);
                    }
                }
            }
            const auto image = view->grabWindow();
            if (config.showClock()) {
                const auto clock = view->rootObject()->property("clockRect").toRectF();
                clocks.append(QJsonObject{{"output"_L1,screen->name()},{"worldX"_L1,clock.x()+view->x()},
                    {"worldY"_L1,clock.y()+view->y()},{"width"_L1,clock.width()},{"height"_L1,clock.height()}});
            }
            QCOMPARE(image.size(), view->geometry().size());
            if (fallback && !fallbackPixels.isEmpty() && index == 0) {
                const auto pixels = image.convertToFormat(QImage::Format_RGB32);
                int bright = 0;
                for (int y = 0; y < pixels.height(); ++y) {
                    const auto* row = reinterpret_cast<const QRgb*>(pixels.constScanLine(y));
                    for (int x = 0; x < pixels.width(); ++x)
                        bright += qGray(row[x]) > fallbackPixels["threshold"_L1].toInt();
                }
                QVERIFY(bright >= fallbackPixels["minimum"_L1].toInt());
                fallbackBrightPixels += bright;
            }
            if (item && !fallback)
                QCOMPARE(item->renderedTimelineNs(), sampleTimeline ? sampledNs : 0);
            const QPoint offset = view->geometry().topLeft() - bounds.topLeft();
            painter.drawImage(offset, image);
            if (!length || index == 0) {
                const auto out = std::find_if(outputs.begin(), outputs.end(),
                    [&](const QJsonValue& value) { return value.toObject()["x"_L1].toInt() == view->x(); });
                QVERIFY(out != outputs.end());
                const auto name = QStringLiteral("%1-%2.png").arg(ns).arg((*out).toObject()["role"_L1].toString());
                QVERIFY(image.save(dir.filePath(name)));
                paths.append(name);
            }
        }
        painter.end();
        if (!length || index == 0) {
            const QString name = QStringLiteral("%1-world.png").arg(ns);
            QVERIFY(world.save(dir.filePath(name)));
            QJsonObject frame{{"timelineNs"_L1,QString::number(ns)},{"outputs"_L1,paths},{"panorama"_L1,name}};
            if (config.showClock()) frame["clocks"_L1]=clocks;
            frames.append(frame);
        }
        if (length) {
            const QImage stitched
                = halfScale ? world.scaled(movieSize, Qt::IgnoreAspectRatio, Qt::SmoothTransformation) : world;
            if (encode) {
                QVERIFY(stream(movie, stitched));
                if (seam)
                    QVERIFY(stream(detail, world.copy(crop)));
            } else {
                QVERIFY(
                    stitched.save(dir.filePath(movieName + QStringLiteral("-%1.png").arg(index, 5, 10, QLatin1Char('0')))));
                if (seam)
                    QVERIFY(world.copy(crop).save(
                        dir.filePath(QStringLiteral("seam-%1.png").arg(index, 5, 10, QLatin1Char('0')))));
            }
        }
        if (length && index % 300 == 0)
            qInfo("Captured frame %d/%d", index, length);
        ++index;
    }
    for (auto* encoder : { &movie, &detail })
        if (encoder->state() != QProcess::NotRunning) {
            encoder->closeWriteChannel();
            QVERIFY(encoder->waitForFinished(60000));
            QCOMPARE(encoder->exitCode(), 0);
        }
    QJsonArray stats;
    for (const auto& s : manager.m_depthCoordinator->statistics())
        stats.append(QJsonObject {
            { "ticks"_L1, qint64(s.ticks) }, { "p99Ns"_L1, qint64(s.p99_ns) }, { "maxNs"_L1, qint64(s.max_ns) } });
    auto manifest = scene;
    QJsonArray swaps;
    for (auto it = kaleidoscopeSwapStarts.cbegin(); it != kaleidoscopeSwapStarts.cend(); ++it) {
        swaps.append(QJsonObject {{"output"_L1,it.key()},
            {"swapStartNs"_L1,QString::number(it.value())},
            {"pourOnsetNs"_L1,QString::number(it.value()+440000000)},
            {"refillStartNs"_L1,QString::number(kaleidoscopeRefillStarts.value(it.key(),it.value()+4000000000LL))},
            {"queueDelaySeconds"_L1,kaleidoscopeQueueDelays.value(it.key())}});
    }
    if (!swaps.isEmpty()) manifest["kaleidoscopeSwaps"_L1] = swaps;
    manifest["frames"_L1] = frames;
    manifest["tickExport"_L1] = stats;
    if (legacy)
        manifest["prewarmSeconds"_L1] = 0;
    manifest["showNs"_L1] = shown - start;
    manifest["mappedNs"_L1] = mapped - start;
    manifest["activationNs"_L1] = activation;
    manifest["worldCount"_L1] = manager.m_depthCoordinator->worldCount();
    manifest["timerStoppedAtActivation"_L1] = config.reducedMotion();
    manifest["videoFrames"_L1] = length;
    if (sampleHz)
        manifest["interpolatedFrames"_L1] = lastPacedFrame + 1;
    manifest["frameCap"_L1] = config.frameRate();
    if (fallback && !fallbackPixels.isEmpty())
        manifest["legacyBrightPixels"_L1] = fallbackBrightPixels;
    if (fallback)
        manifest["fallbackPaintNs"_L1] = fallbackPaintNs;
    QFile report(dir.filePath(QStringLiteral("manifest.json")));
    QVERIFY(report.open(QIODevice::WriteOnly));
    report.write(QJsonDocument(manifest).toJson());
    report.close();
    manager.hide();
    QTRY_COMPARE_WITH_TIMEOUT(manager.m_pendingViewDeletions, 0, 10000);
}

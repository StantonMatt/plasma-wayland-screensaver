// SPDX-License-Identifier: GPL-3.0-or-later
#include "visuals/constellation/primitivebatch.h"
#include <QTest>
#include <numeric>

namespace {
thread_local bool countAllocations = false;
thread_local size_t allocations = 0;
}
// Qt's QByteArray and geometry use the C allocator directly.
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

class ConstellationUploadTest final : public QObject {
    Q_OBJECT
private Q_SLOTS:
    void warmedPresentationsDoNotAllocate()
    {
        const vd_config config { 2, 0, 1, 1., 50., 1., .35, 0, 0 };
        const vd_output output { 1, { 0, 0, 2560, 1440 }, 1., 60., 1, 0 };
        const vd_environment environment {};
        VisualWorld world(config, {}, { output }, environment, {}, 1, 50'000'000);
        QVERIFY(world.valid());
        PrimitiveBatch batch;
        vd_view view { 1, 1, 0, output.viewport, 1., 0., 0., 1. };
        std::array<size_t, 120> perPresentation {};
        size_t coldAllocations = 0;
        size_t changedPresentations = 0;
        QByteArray previous;
        for (int frame = 0; frame < 130; ++frame) {
            view.timeline_ns = int64_t(frame) * 1'000'000'000 / 60;
            QVERIFY(world.ensureThrough(view.timeline_ns));
            auto lease = world.sample(view);
            QVERIFY(lease);
            vd_sample_info info {};
            QCOMPARE(visuals_depth_sample_info(lease.get(), &info), 0);
            // Confirm the measured presentations contain changing record bytes.
            std::array<visuals_core_primitive, 4096> records {};
            auto count = visuals_constellation_primitives(lease.get(), false, records.data(), records.size());
            QVERIFY(count > 0 && count <= records.size());
            QByteArray current(reinterpret_cast<const char*>(records.data()), qsizetype(count * sizeof(records[0])));
            if (frame >= 10 && current != previous) ++changedPresentations;
            previous = std::move(current);

            allocations = 0;
            countAllocations = true;
            const bool uploaded = batch.sync(lease, info);
            const bool repeatedUpload = batch.sync(lease, info);
            countAllocations = false;
            if (!frame) coldAllocations = allocations;
            if (frame >= 10) perPresentation[frame - 10] = allocations;
            QVERIFY(uploaded);
            QVERIFY(!repeatedUpload);
            QCOMPARE(static_cast<VisualMaterial*>(batch.material())->primitiveCount, uint32_t(count));
        }
        QVERIFY(coldAllocations > 0);
        QCOMPARE(changedPresentations, size_t(120));
        const auto total = std::accumulate(perPresentation.begin(), perPresentation.end(), size_t(0));
        qInfo("Constellation upload: cold=%zu; changed presentations=%zu; warmed allocations=%zu; max per presentation=%zu",
            coldAllocations, changedPresentations, total, *std::max_element(perPresentation.begin(), perPresentation.end()));
        QCOMPARE(total, size_t(0));
        vd_sample_info info {};
        info.view = view;
        allocations = 0;
        countAllocations = true;
        const bool emptied = batch.sync({}, info);
        const bool repeatedEmpty = batch.sync({}, info);
        countAllocations = false;
        QVERIFY(emptied);
        QVERIFY(!repeatedEmpty);
        QCOMPARE(allocations, size_t(0));
        QCOMPARE(static_cast<VisualMaterial*>(batch.material())->primitiveCount, 0U);
    }
};
QTEST_GUILESS_MAIN(ConstellationUploadTest)
#include "test_upload.moc"

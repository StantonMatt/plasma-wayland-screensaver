// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include "visualmaterial.h"
#include "visualworld.h"
#include "visuals_depth_constellation.h"
#include <QSGGeometryNode>
#include <algorithm>
#include <array>
#include <bit>
#include <cstring>
#include <vector>

namespace {
class PrimitiveBatch : public QSGGeometryNode {
public:
    PrimitiveBatch()
        : records(4096)
    {
        setGeometry(new QSGGeometry(QSGGeometry::defaultAttributes_TexturedPoint2D(), 0));
        geometry()->setDrawingMode(QSGGeometry::DrawTriangles);
        setFlag(OwnsGeometry);
        setMaterial(new VisualMaterial(VisualMaterial::PrimitiveTexture));
        setFlag(OwnsMaterial);
        for (auto& buffer : uploads)
            buffer.reserve(qsizetype(records.size() * sizeof(records[0])));
    }

    bool sync(const VisualSampleLease& lease, const vd_sample_info& info)
    {
        auto count = visuals_constellation_primitives(lease.get(), false, records.data(), records.size());
        if (count > records.size()) {
            records.resize(count);
            visuals_constellation_primitives(lease.get(), false, records.data(), records.size());
        }
        auto* m = static_cast<VisualMaterial*>(material());
        const auto capacity = std::max(m->recordCapacity, int(std::bit_ceil(std::max(size_t(1), count))));
        const QSizeF viewport(info.view.viewport.width, info.view.viewport.height);
        const bool geometryChanged = m->recordCapacity != capacity || m->primitiveViewport != viewport;
        if (geometryChanged) {
            geometry()->allocate(capacity * 6);
            constexpr float corners[][2] = { { 0, 0 }, { 1, 0 }, { 0, 1 }, { 0, 1 }, { 1, 0 }, { 1, 1 } };
            auto* vertex = geometry()->vertexDataAsTexturedPoint2D();
            for (int i = 0; i < capacity; ++i)
                for (const auto& corner : corners)
                    (vertex++)->set(
                        float(viewport.width()) * corner[0], float(viewport.height()) * corner[1], float(i), 0);
            markDirty(DirtyGeometry);
        }
        const float fringe = float(1. / info.view.dpr);
        const bool materialChanged = geometryChanged || m->primitiveCount != count || m->fringe != fringe;
        m->primitiveCount = uint32_t(count);
        m->recordCapacity = capacity;
        m->primitiveViewport = viewport;
        m->fringe = fringe;

        const visuals_core_primitive empty {};
        const char* data = reinterpret_cast<const char*>(count ? records.data() : &empty);
        const auto bytes = qsizetype(std::max(size_t(1), count) * sizeof(records[0]));
        if (lastUpload < 0 || uploads[lastUpload].size() != bytes
            || std::memcmp(uploads[lastUpload].constData(), data, size_t(bytes)) != 0
            || m->dataTexture->textureSize() != QSize(3, capacity)) {
            // The texture and QRhi keep owning QByteArray references until the
            // resource update is consumed. Alternate storage so the texture's
            // last data is never detached every frame. Qt Quick consumes the
            // preceding frame's resource updates before the next node sync;
            // QByteArray's COW also preserves any longer-lived upload reference.
            const int next = lastUpload < 0 ? 0 : 1 - lastUpload;
            auto& buffer = uploads[next];
            buffer.resize(bytes);
            std::memcpy(buffer.data(), data, size_t(bytes));
            m->dataTexture->setData(buffer, { 3, capacity }, { 3, int(std::max(size_t(1), count)) });
            lastUpload = next;
            markDirty(DirtyMaterial);
            return true;
        } else if (materialChanged) {
            markDirty(DirtyMaterial);
        }
        return false;
    }

private:
    std::vector<visuals_core_primitive> records;
    std::array<QByteArray, 2> uploads;
    int lastUpload = -1;
};
}

// SPDX-License-Identifier: GPL-3.0-or-later
#include "livingvisualitem.h"
#include "visualdepthregistry.h"
#include "visualmaterial.h"
#include "visuals_depth_constellation.h"
#include "primitivebatch.h"
#include <QQuickWindow>
#include <QSGGeometryNode>
#include <QSGSimpleTextureNode>
#include <QSGTransformNode>
#include <QSGVertexColorMaterial>

#include <vector>

namespace {
// Far dust is immutable between layout revisions. A small radial colour fan
// gives each speck subpixel coverage without a record-texture size limit.
class DustNode : public QSGGeometryNode {
public:
    DustNode()
    {
        setGeometry(new QSGGeometry(QSGGeometry::defaultAttributes_ColoredPoint2D(), 0));
        geometry()->setDrawingMode(QSGGeometry::DrawTriangles);
        setFlag(OwnsGeometry);
        setMaterial(new QSGVertexColorMaterial);
        setFlag(OwnsMaterial);
    }

    void sync(const VisualSampleLease& lease)
    {
        std::vector<visuals_core_primitive> records(512);
        const auto count = visuals_constellation_primitives(lease.get(), true, records.data(), records.size());
        if (count > records.size()) {
            records.resize(count);
            visuals_constellation_primitives(lease.get(), true, records.data(), records.size());
        }
        geometry()->allocate(int(count) * 12);
        auto* v = geometry()->vertexDataAsColoredPoint2D();
        constexpr float corners[][2] = { { 1, 0 }, { 0, 1 }, { -1, 0 }, { 0, -1 }, { 1, 0 } };
        for (size_t i = 0; i < count; ++i) {
            const auto& rec = records[i];
            const float x = rec.geometry[0], y = rec.geometry[1], r = std::max(0.92f, rec.shape[0] * 1.65f);
            const float alpha = rec.color[3] * 255.f;
            for (int side = 0; side < 4; ++side) {
                (v++)->set(x, y, uchar(rec.color[0] * alpha), uchar(rec.color[1] * alpha),
                    uchar(rec.color[2] * alpha), uchar(alpha));
                for (int j = side; j <= side + 1; ++j)
                    (v++)->set(x + r * corners[j][0], y + r * corners[j][1], 0, 0, 0, 0);
            }
        }
        markDirty(DirtyGeometry);
    }
};

class SkyNode : public QSGNode {
public:
    QSGTransformNode* far = new QSGTransformNode;
    DustNode* dust = new DustNode;
    PrimitiveBatch* sky = new PrimitiveBatch;
    QSGSimpleTextureNode* band = nullptr;
    uint64_t layout = 0;
    SkyNode()
    {
        appendChildNode(far);
        far->appendChildNode(dust);
        appendChildNode(sky);
    }
};

class SkyItem final : public LivingVisualItem {
public:
    using LivingVisualItem::LivingVisualItem;

protected:
    QSGNode* buildDepthNode(QSGNode* old, const VisualSampleLease& lease, const vd_sample_info& info,
        const vd_endpoint&, const vd_endpoint& upper) override
    {
        auto* node = static_cast<SkyNode*>(old);
        if (!node) {
            if (!VisualMaterial::supportsWindow(window(), VisualMaterial::PrimitiveTexture, 4096)) {
                Q_EMIT gpuUnavailable();
                return nullptr;
            }
            qInfo("Visuals Constellation: living atlas GPU primitive records");
            node = new SkyNode;
        }
        if (info.generations.layout != node->layout) {
            node->layout = info.generations.layout;
            if (node->band) {
                delete node->band;
                node->band = nullptr;
            }
            if (upper.texture_width && upper.texture_height) {
                const QImage image(upper.texture_bytes, int(upper.texture_width), int(upper.texture_height),
                    QImage::Format_RGBA8888);
                auto* texture = window()->createTextureFromImage(image.copy());
                if (!texture) {
                    delete node;
                    Q_EMIT gpuUnavailable();
                    return nullptr;
                }
                node->band = new QSGSimpleTextureNode;
                node->band->setTexture(texture);
                node->band->setOwnsTexture(true);
                node->band->setFiltering(QSGTexture::Linear);
                const auto& b = info.world_bounds;
                const auto& v = info.view;
                node->band->setRect((b.x - v.viewport.x) * v.uniform_scale + v.offset_x,
                    (b.y - v.viewport.y) * v.uniform_scale + v.offset_y, b.width * v.uniform_scale,
                    b.height * v.uniform_scale);
                node->far->prependChildNode(node->band);
            }
            node->dust->sync(lease);
        }
        float offset[2] {};
        visuals_constellation_parallax(lease.get(), offset);
        QMatrix4x4 transform;
        transform.translate(offset[0], offset[1]);
        node->far->setMatrix(transform);
        node->sky->sync(lease, info);
        return node;
    }
};
}
void registerDepthConstellationAdapter()
{
    VisualDepthRegistry::registerItemFactory(2, [](QQuickItem* parent) { return new SkyItem(parent); });
}

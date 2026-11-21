// SPDX-License-Identifier: GPL-3.0-or-later
#include "livingvisualitem.h"
#include "visualdepthregistry.h"
#include "visualmaterial.h"
#include "visuals_depth_orbs.h"
#include <QFile>
#include <QQuickWindow>
#include <QSGGeometryNode>
#include <QSGMaterialShader>
#include <bit>
#include <cstring>
#include <limits>
#include <rhi/qshader.h>

static void initOrbsShaders() { Q_INIT_RESOURCE(depth_orbs_material); }

namespace {
struct Shaders {
    QShader vertex, fragment;
    Shaders()
    {
        initOrbsShaders();
        auto load = [](const char* name) {
            QFile f(QStringLiteral(":/depth/orbs/") + QString::fromLatin1(name));
            return f.open(QIODevice::ReadOnly) ? QShader::fromSerialized(f.readAll()) : QShader {};
        };
        vertex = load("orbs.vert.qsb");
        fragment = load("orbs.frag.qsb");
    }
};
const Shaders& shaders()
{
    static const Shaders value;
    return value;
}

class OrbMaterial final : public QSGMaterial {
public:
    VisualDataTexture records;
    vd_orb_style style {};
    vd_view view {};
    int count = 0, capacity = 0;
    OrbMaterial()
    {
        setFlag(Blending);
        setFlag(NoBatching);
        setFlag(RequiresFullMatrix);
    }
    QSGMaterialType* type() const override
    {
        static QSGMaterialType type;
        return &type;
    }
    QSGMaterialShader* createShader(QSGRendererInterface::RenderMode) const override;
};
class OrbShader final : public QSGMaterialShader {
public:
    OrbShader()
    {
        setFlag(UpdatesGraphicsPipelineState);
        setShader(VertexStage, shaders().vertex);
        setShader(FragmentStage, shaders().fragment);
    }
    bool updateGraphicsPipelineState(RenderState&, GraphicsPipelineState* state, QSGMaterial* material,
        QSGMaterial*) override
    {
        const bool emitting = static_cast<OrbMaterial*>(material)->style.look < 0.5f;
        // Add light in each channel's remaining headroom. This optical sum is
        // order independent and cannot darken a previously drawn surface.
        state->srcColor = emitting ? GraphicsPipelineState::OneMinusDstColor : GraphicsPipelineState::One;
        state->dstColor = emitting ? GraphicsPipelineState::One : GraphicsPipelineState::OneMinusSrcAlpha;
        state->separateBlendFactors = true;
        state->srcAlpha = GraphicsPipelineState::One;
        state->dstAlpha = GraphicsPipelineState::OneMinusSrcAlpha;
        return true;
    }
    bool updateUniformData(RenderState& state, QSGMaterial* material, QSGMaterial*) override
    {
        auto* m = static_cast<OrbMaterial*>(material);
        auto* data = state.uniformData()->data();
        if (state.isMatrixDirty())
            std::memcpy(data, state.combinedMatrix().constData(), 64);
        const auto& v = m->view;
        const float values[] = { state.opacity(), float(m->count), float(m->capacity), float(v.uniform_scale * v.dpr),
            m->style.look, m->style.glow, m->style.phase, m->style.blend, float(v.viewport.width),
            float(v.viewport.height), float(v.viewport.x), float(v.viewport.y), float(v.uniform_scale),
            float(v.offset_x), float(v.offset_y), m->style.palette };
        std::memcpy(data + 64, values, sizeof(values));
        return true;
    }
    void updateSampledImage(
        RenderState& state, int binding, QSGTexture** texture, QSGMaterial* material, QSGMaterial*) override
    {
        if (binding == 1) {
            *texture = &static_cast<OrbMaterial*>(material)->records;
            (*texture)->commitTextureOperations(state.rhi(), state.resourceUpdateBatch());
        }
    }
};
QSGMaterialShader* OrbMaterial::createShader(QSGRendererInterface::RenderMode) const { return new OrbShader; }

class OrbBatch final : public QSGGeometryNode {
    uint64_t lower = std::numeric_limits<uint64_t>::max(), upper = 0, layout = 0;
    int storage = 128;
    QByteArray staging[2];
    unsigned nextBuffer = 0;

public:
    OrbBatch()
    {
        auto* g = new QSGGeometry(QSGGeometry::defaultAttributes_TexturedPoint2D(), 0);
        g->setDrawingMode(QSGGeometry::DrawTriangles);
        setGeometry(g);
        setFlag(OwnsGeometry);
        setMaterial(new OrbMaterial);
        setFlag(OwnsMaterial);
    }
    void sync(const VisualSampleLease& lease, const vd_sample_info& info)
    {
        auto* m = static_cast<OrbMaterial*>(material());
        visuals_orbs_style(lease.get(), &m->style);
        if (lower != info.lower_tick || upper != info.upper_tick || layout != info.generations.layout) {
            auto& bytes = staging[nextBuffer++ % 2];
            bytes.resizeForOverwrite(storage * 2 * int(sizeof(vd_orb_record)));
            auto write = [&]() {
                return visuals_orbs_records(
                    lease.get(), reinterpret_cast<vd_orb_record*>(bytes.data()), size_t(storage));
            };
            auto count = write();
            if (count > size_t(storage)) {
                storage = int(std::bit_ceil(count));
                bytes.resizeForOverwrite(storage * 2 * int(sizeof(vd_orb_record)));
                write();
            }
            const QSize size(6, std::max(1, storage));
            const int rows = std::max(1, int(count));
            if (!count)
                std::memset(bytes.data(), 0, 2 * sizeof(vd_orb_record));
            bytes.truncate(rows * 2 * int(sizeof(vd_orb_record)));
            m->records.setData(bytes, size, { 6, rows });
            if (m->capacity != storage || m->view.viewport.width != info.view.viewport.width
                || m->view.viewport.height != info.view.viewport.height) {
                geometry()->allocate(storage * 6);
                auto* v = geometry()->vertexDataAsTexturedPoint2D();
                constexpr float corners[][2] = { { 0, 0 }, { 1, 0 }, { 0, 1 }, { 0, 1 }, { 1, 0 }, { 1, 1 } };
                for (int i = 0; i < storage; ++i)
                    for (const auto& xy : corners)
                        (v++)->set(xy[0] * float(info.view.viewport.width), xy[1] * float(info.view.viewport.height),
                            float(i), 0.f);
                markDirty(DirtyGeometry);
            }
#if QT_VERSION >= QT_VERSION_CHECK(6, 10, 0)
            if (geometry()->vertexCount() != int(count) * 6) {
                geometry()->setVertexCount(int(count) * 6);
                markDirty(DirtyGeometry);
            }
#endif
            m->capacity = storage;
            m->count = int(count);
            lower = info.lower_tick;
            upper = info.upper_tick;
            layout = info.generations.layout;
        }
        m->view = info.view;
        markDirty(DirtyMaterial);
    }
};
class OrbsItem final : public LivingVisualItem {
    bool suppliedClock = false;

public:
    using LivingVisualItem::LivingVisualItem;
    void setSample(VisualSampleLease lease) override
    {
        double xy[2];
        const bool hasClock = lease && visuals_orbs_clock(lease.get(), xy);
        if ((hasClock || suppliedClock) && parentItem() && parentItem()->parentItem())
            parentItem()->parentItem()->setProperty(
                "livingClockPosition", hasClock ? QPointF(xy[0], xy[1]) : QPointF(-1e9, -1e9));
        suppliedClock = hasClock;
        LivingVisualItem::setSample(std::move(lease));
    }

protected:
    QSGNode* buildDepthNode(QSGNode* old, const VisualSampleLease& lease, const vd_sample_info& info,
        const vd_endpoint&, const vd_endpoint&) override
    {
        auto* node = static_cast<OrbBatch*>(old);
        if (!node) {
            if (!VisualMaterial::supportsRecordShaders(window(), shaders().vertex, shaders().fragment, { 6, 4096 })) {
                Q_EMIT gpuUnavailable();
                return nullptr;
            }
            qInfo("Visuals Orbs: living GPU material with interpolated sphere records");
            node = new OrbBatch;
        }
        node->sync(lease, info);
        return node;
    }
};
} // namespace
void registerDepthOrbsAdapter()
{
    VisualDepthRegistry::registerItemFactory(8, [](QQuickItem* parent) { return new OrbsItem(parent); });
}

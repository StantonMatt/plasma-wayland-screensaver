// SPDX-License-Identifier: GPL-3.0-or-later
#include "livingvisualitem.h"
#include "visualdepthregistry.h"
#include "visuals_depth_fireflies.h"
#include <QFile>
#include <QQuickWindow>
#include <QSGRenderNode>
#include <algorithm>
#include <array>
#include <cstring>
#include <limits>
#include <rhi/qrhi.h>

static void initializeMeadowShaders() { Q_INIT_RESOURCE(depth_fireflies_shaders); }

namespace {
struct Uniforms {
    float matrix[16], view[4], bounds[4];
    int counts[4], rows[4];
    float art[4], colours[8][4], bottom[4], limits[4];
};
static_assert(sizeof(Uniforms) == 304);

const std::array<QShader, 2>& shaders()
{
    static const auto result = [] {
        initializeMeadowShaders();
        auto load = [](const QString& name) {
            QFile file(QStringLiteral(":/fireflies/meadow.") + name + QStringLiteral(".qsb"));
            return file.open(QIODevice::ReadOnly) ? QShader::fromSerialized(file.readAll()) : QShader {};
        };
        return std::array { load(QStringLiteral("vert")), load(QStringLiteral("frag")) };
    }();
    return result;
}

class MeadowNode final : public QSGRenderNode {
public:
    bool sync(QQuickWindow* window, const VisualSampleLease& lease, const vd_sample_info& info)
    {
        vd_firefly_frame frame {};
        if (!visuals_fireflies_frame(lease.get(), &frame))
            return false;
        auto* ri = window->rendererInterface();
        auto* rhi = static_cast<QRhi*>(ri->getResource(window, QSGRendererInterface::RhiResource));
        if (!rhi || rhi->backend() == QRhi::Null || !rhi->isFeatureSupported(QRhi::Instancing)
            || !rhi->isTextureFormatSupported(QRhiTexture::RGBA32F))
            return false;
        auto* target = static_cast<QRhiRenderTarget*>(
            ri->getResource(window, QSGRendererInterface::RhiRedirectRenderTarget));
        if (!target) {
            auto* chain
                = static_cast<QRhiSwapChain*>(ri->getResource(window, QSGRendererInterface::RhiSwapchainResource));
            if (chain)
                target = chain->currentFrameRenderTarget();
        }
        if (!target || !target->renderPassDescriptor())
            return false;
        const uint32_t width = std::max(frame.upper.length, frame.grass.length);
        const int bladeRows = int((frame.blades.length + width - 1) / width);
        const QSize size(int(width), 5 + int(frame.ring_length) + bladeRows);
        if (!m_pipeline || size != m_atlas->pixelSize() || rhi != m_rhi || m_generation != info.generations.layout
            || !m_pass->isCompatible(target->renderPassDescriptor())
            || target->sampleCount() != m_pipeline->sampleCount()) {
            releaseResources();
            if (!initialize(rhi, target, size))
                return false;
        }
        if (m_info.generations.topology != info.generations.topology)
            m_first = true;
        m_generation = info.generations.layout;
        m_lease = lease;
        m_info = info;
        m_frame = frame;
        markDirty(DirtyForceUpdate);
        return true;
    }

    void prepare() override
    {
        if (!m_pipeline || !m_lease || !commandBuffer())
            return;
        const auto& f = m_frame;
        const auto& v = m_info.view;
        const int width = m_atlas->pixelSize().width();
        auto* updates = m_rhi->nextResourceUpdateBatch();
        // On the first frame every row is known. A single image transfer avoids
        // issuing a separate driver upload for each ring slot and blade row.
        QByteArray firstImage;
        if (m_first)
            firstImage.fill(0, qsizetype(width) * m_atlas->pixelSize().height() * 16);
        const auto upload = [&](const float (*ptr)[4], int count, int row) {
            if (!count)
                return;
            if (m_first) {
                std::memcpy(firstImage.data() + qsizetype(row) * width * 16, ptr, size_t(count) * 16);
                return;
            }
            QRhiTextureSubresourceUploadDescription part(
                QByteArray(reinterpret_cast<const char*>(ptr), count * 16));
            part.setSourceSize({ count, 1 });
            part.setDestinationTopLeft({ 0, row });
            updates->uploadTexture(
                m_atlas.get(), QRhiTextureUploadDescription({ QRhiTextureUploadEntry(0, 0, part) }));
        };
        if (m_first) {
            upload(f.terrain.texels, int(f.terrain.length), m_atlas->pixelSize().height() - 1);
            for (uint32_t n = 0; n < f.blades.length; n += width)
                upload(f.blades.texels + n, int(std::min(uint32_t(width), f.blades.length - n)),
                    4 + int(f.ring_length + n / width));
        }
        // Keep two physical head rows keyed by tick parity. Mixed-refresh outputs
        // can skip ticks or seek arbitrarily without modifying any other window.
        const int lower = int(f.lower_serial % 2), upper = int(f.upper_serial % 2);
        if (m_first || m_cachedHead[lower] != f.lower_serial) {
            upload(f.lower.texels, int(f.lower.length), lower);
            upload(f.grass_lower.texels, int(f.grass_lower.length), lower + 2);
            m_cachedHead[lower] = f.lower_serial;
        }
        if (m_first || m_cachedHead[upper] != f.upper_serial) {
            upload(f.upper.texels, int(f.upper.length), upper);
            upload(f.grass.texels, int(f.grass.length), upper + 2);
            m_cachedHead[upper] = f.upper_serial;
        }
        if (m_first || m_historySerial != f.history_serial) {
            const auto changed = m_first || f.history_serial < m_historySerial
                ? f.ring_length
                : uint32_t(std::min(uint64_t(f.ring_length), f.history_serial - m_historySerial));
            for (uint32_t age = 0; age < changed; ++age) {
                const uint32_t slot = (f.history_serial + f.ring_length - age) % f.ring_length;
                auto row = f.history[slot];
                if (row.length)
                    upload(row.texels, int(f.mid_count), 4 + int(slot));
                else if (!m_first) {
                    QByteArray zero(int(f.mid_count) * 16, 0);
                    upload(reinterpret_cast<const float (*)[4]>(zero.constData()), int(f.mid_count), 4 + int(slot));
                }
            }
            m_historySerial = f.history_serial;
        }
        if (m_first) {
            QRhiTextureSubresourceUploadDescription image(firstImage);
            image.setSourceSize(m_atlas->pixelSize());
            updates->uploadTexture(
                m_atlas.get(), QRhiTextureUploadDescription({ QRhiTextureUploadEntry(0, 0, image) }));
        }
        m_first = false;
        Uniforms u {};
        const auto m = *projectionMatrix() * *matrix();
        std::memcpy(u.matrix, m.constData(), 64);
        u.view[0] = float(v.offset_x - v.viewport.x * v.uniform_scale);
        u.view[1] = float(v.offset_y - v.viewport.y * v.uniform_scale);
        u.view[2] = float(v.uniform_scale);
        u.view[3] = float(m_info.alpha);
        u.bounds[0] = float(v.viewport.x - v.offset_x / v.uniform_scale);
        u.bounds[1] = float(v.viewport.y - v.offset_y / v.uniform_scale);
        u.bounds[2] = u.bounds[0] + float(v.viewport.width / v.uniform_scale);
        u.bounds[3] = u.bounds[1] + float(v.viewport.height / v.uniform_scale);
        u.counts[0] = int(f.blades.length);
        u.counts[1] = int(f.mid_count);
        u.counts[2] = int(f.trail_segments);
        u.counts[3] = width;
        u.rows[0] = lower;
        u.rows[1] = upper;
        u.rows[2] = int(f.history_serial % f.ring_length);
        u.rows[3] = int(f.ring_length);
        std::memcpy(u.art, f.art, sizeof(u.art));
        std::memcpy(u.colours, f.colours, sizeof(u.colours));
        std::memcpy(u.bottom, f.bottom, sizeof(u.bottom));
        u.limits[0] = float(f.grass.length);
        u.limits[1] = float(inheritedOpacity());
        u.limits[2] = float(f.still);
        u.limits[3] = float((f.lower_serial % 3 + m_info.alpha) / 3.0);
        updates->updateDynamicBuffer(m_uniforms.get(), 0, sizeof(u), &u);
        commandBuffer()->resourceUpdate(updates);
    }
    void render(const RenderState* state) override
    {
        if (!m_pipeline || !commandBuffer())
            return;
        auto* cb = commandBuffer();
        const auto size = renderTarget()->pixelSize();
        cb->setGraphicsPipeline(m_pipeline.get());
        cb->setViewport({ 0, 0, float(size.width()), float(size.height()) });
        const QRect clip = state->scissorEnabled() ? state->scissorRect() : QRect(QPoint(), size);
        cb->setScissor({ clip.x(), clip.y(), clip.width(), clip.height() });
        cb->setShaderResources(m_bindings.get());
        // Four centripetal Catmull-Rom subsegments per 100 ms history span.
        const auto ribbonQuads = 4 * m_frame.mid_count * m_frame.trail_segments;
        cb->draw(4, 1 + m_frame.blades.length + ribbonQuads + m_frame.upper.length);
    }
    RenderingFlags flags() const override { return NoExternalRendering | BoundedRectRendering; }
    StateFlags changedStates() const override { return ViewportState | ScissorState | BlendState; }
    QRectF rect() const override { return { 0, 0, m_info.view.viewport.width, m_info.view.viewport.height }; }
    void releaseResources() override
    {
        m_pipeline.reset();
        m_pass.reset();
        m_bindings.reset();
        m_uniforms.reset();
        m_sampler.reset();
        m_atlas.reset();
        m_lease = {};
        m_rhi = nullptr;
        m_first = true;
        m_cachedHead.fill(UINT64_MAX);
    }

private:
    bool initialize(QRhi* rhi, QRhiRenderTarget* target, QSize size)
    {
        const auto& packs = shaders();
        if (!packs[0].isValid() || !packs[1].isValid() || size.width() > rhi->resourceLimit(QRhi::TextureSizeMax)
            || size.height() > rhi->resourceLimit(QRhi::TextureSizeMax))
            return false;
        m_rhi = rhi;
        m_atlas.reset(rhi->newTexture(QRhiTexture::RGBA32F, size));
        m_sampler.reset(rhi->newSampler(QRhiSampler::Nearest, QRhiSampler::Nearest, QRhiSampler::None,
            QRhiSampler::ClampToEdge, QRhiSampler::ClampToEdge));
        m_uniforms.reset(rhi->newBuffer(QRhiBuffer::Dynamic, QRhiBuffer::UniformBuffer, sizeof(Uniforms)));
        if (!m_atlas->create() || !m_sampler->create() || !m_uniforms->create())
            return false;
        m_bindings.reset(rhi->newShaderResourceBindings());
        using B = QRhiShaderResourceBinding;
        m_bindings->setBindings({ B::uniformBuffer(0, B::VertexStage | B::FragmentStage, m_uniforms.get()),
            B::sampledTexture(1, B::VertexStage, m_atlas.get(), m_sampler.get()) });
        if (!m_bindings->create())
            return false;
        m_pass.reset(target->renderPassDescriptor()->newCompatibleRenderPassDescriptor());
        m_pipeline.reset(rhi->newGraphicsPipeline());
        m_pipeline->setShaderStages(
            { { QRhiShaderStage::Vertex, packs[0] }, { QRhiShaderStage::Fragment, packs[1] } });
        m_pipeline->setTopology(QRhiGraphicsPipeline::TriangleStrip);
        m_pipeline->setShaderResourceBindings(m_bindings.get());
        m_pipeline->setRenderPassDescriptor(m_pass.get());
        m_pipeline->setSampleCount(target->sampleCount());
        m_pipeline->setFlags(QRhiGraphicsPipeline::UsesScissor);
        QRhiGraphicsPipeline::TargetBlend blend;
        blend.enable = true;
        blend.srcColor = blend.srcAlpha = QRhiGraphicsPipeline::One;
        blend.dstColor = blend.dstAlpha = QRhiGraphicsPipeline::OneMinusSrcAlpha;
        m_pipeline->setTargetBlends({ blend });
        return m_pipeline->create();
    }
    QRhi* m_rhi = nullptr;
    std::unique_ptr<QRhiTexture> m_atlas;
    std::unique_ptr<QRhiSampler> m_sampler;
    std::unique_ptr<QRhiBuffer> m_uniforms;
    std::unique_ptr<QRhiShaderResourceBindings> m_bindings;
    std::unique_ptr<QRhiRenderPassDescriptor> m_pass;
    std::unique_ptr<QRhiGraphicsPipeline> m_pipeline;
    bool m_first = true;
    std::array<uint64_t, 2> m_cachedHead { UINT64_MAX, UINT64_MAX };
    uint64_t m_generation = 0, m_historySerial = 0;
    VisualSampleLease m_lease;
    vd_sample_info m_info {};
    vd_firefly_frame m_frame {};
};
class MeadowItem final : public LivingVisualItem {
public:
    explicit MeadowItem(QQuickItem* parent)
        : LivingVisualItem(parent)
    {
        // Decode once while the windows map, before their render threads need
        // the shader packages at the same time. GPU resources stay per window.
        (void)shaders();
    }

protected:
    QSGNode* buildDepthNode(QSGNode* old, const VisualSampleLease& lease, const vd_sample_info& info,
        const vd_endpoint&, const vd_endpoint&) override
    {
        auto* node = static_cast<MeadowNode*>(old);
        if (!node)
            node = new MeadowNode;
        if (!node->sync(window(), lease, info)) {
            delete node;
            Q_EMIT gpuUnavailable();
            return nullptr;
        }
        if (!old)
            qInfo("Visuals Fireflies: living meadow, instanced GPU atlas");
        return node;
    }
};
}
void registerDepthFirefliesAdapter()
{
    VisualDepthRegistry::registerItemFactory(9, [](QQuickItem* parent) { return new MeadowItem(parent); });
}

// SPDX-License-Identifier: GPL-3.0-or-later
#include "fold.h"
#include "blur.h"
#include "livingvisualitem.h"
#include "visualdepthregistry.h"
#include "visuals/common/offscreentarget.h"
#include <QSGRenderNode>
#include <algorithm>
#include <array>
#include <cstring>
#include <cmath>

namespace
{
using Buffer = std::unique_ptr<QRhiBuffer>;
using Bindings = std::unique_ptr<QRhiShaderResourceBindings>;
using Pipeline = std::unique_ptr<QRhiGraphicsPipeline>;
constexpr int MaxPieces = 65; // 64 bodies and the optional mote

class ChamberNode final : public QSGRenderNode
{
public:
    explicit ChamberNode(QQuickWindow* window)
        : m_window(window)
    {
    }
    ~ChamberNode() override
    {
        releaseResources();
    }

    bool sync(const VisualSampleLease& lease, const vd_sample_info& info)
    {
        m_count = visuals_depth_kaleidoscope_sample(lease.get(), &m_lens, m_poses.data(), m_poses.size());
        if (!m_count || m_count > MaxPieces)
            return false;
        const auto& v = info.view;
        const QSizeF logical(v.viewport.width, v.viewport.height);
        const int width = std::clamp(qRound(1.55 * m_lens.eye[2] * v.dpr * std::sqrt(m_lens.glass[1])), 256, 1536);
        const QSize size(width, qRound(width * 0.6));
        if (!m_targets[0] || size != m_targets[0]->target()->pixelSize() || m_logical != logical) {
            releaseResources();
            if (!setup(size))
                return false;
            m_logical = logical;
            m_fold = new FoldNode(m_targets[0]->texture(), m_targets[2]->texture(), logical);
            appendChildNode(m_fold);
            qInfo("Visuals Kaleidoscope: living glass chamber %dx%d, %zu records", size.width(), size.height(),
                m_count);
        }
        if (m_revision != info.generations.data || m_layout != info.generations.layout) {
            size_t count = 0;
            const auto* cuts = visuals_depth_kaleidoscope_templates(lease.get(), &count);
            if (!cuts || count != m_count)
                return false;
            std::memcpy(m_cuts.data(), cuts, count * sizeof(*cuts));
            m_revision = info.generations.data;
            m_layout = info.generations.layout;
            m_cutsDirty = true;
        }
        m_fold->sync(m_lens);
        // During a Hold, the native fold still drifts. Reuse the chamber and
        // bloom until a body, the mote, or the quantized key light changes.
        const std::array<int, 4> key { qRound(m_lens.light[0] * 4096), qRound(m_lens.light[1] * 4096),
            qRound(m_lens.light[2] * 256), qRound(m_lens.light[3] * 256) };
        const bool blurDirty = m_blurState.sync(m_lens);
        m_pending |= m_cutsDirty || m_initialize || blurDirty || key != m_lightKey
            || std::memcmp(m_poses.data(), m_previous.data(), m_count * sizeof(m_poses[0])) != 0;
        if (m_pending) {
            m_lightKey = key;
            m_previous = m_poses;
            m_lens.light[0] = key[0] / 4096.f;
            m_lens.light[1] = key[1] / 4096.f;
            m_lens.light[2] = key[2] / 256.f;
            m_lens.light[3] = key[3] / 256.f;
        }
        markDirty(DirtyForceUpdate);
        return true;
    }
    void prepare() override
    {
        auto* cb = commandBuffer();
        if (!cb || !m_pending)
            return;
        auto* updates = m_rhi->nextResourceUpdateBatch();
        if (m_initialize) {
            constexpr float corners[] = { 0, 0, 1, 0, 0, 1, 1, 1 };
            updates->uploadStaticBuffer(m_quad.get(), corners);
            const float flip = m_rhi->isYUpInNDC() == m_rhi->isYUpInFramebuffer() ? 1.f : -1.f;
            const float target[4] = { flip, 0, 0, 0 };
            updates->updateDynamicBuffer(m_scene.get(), sizeof(m_lens), sizeof(target), target);
            m_initialize = false;
        }
        const float flip = m_rhi->isYUpInNDC() == m_rhi->isYUpInFramebuffer() ? 1.f : -1.f;
        if (const auto uniforms = m_blurState.take(m_targets[0]->target()->pixelSize(), flip)) {
            for (size_t i = 0; i < uniforms->size(); ++i)
                updates->uploadStaticBuffer(m_blurUniforms[i].get(), (*uniforms)[i].data());
        }
        if (m_cutsDirty) {
            updates->uploadStaticBuffer(m_templates.get(), 0, quint32(m_count * sizeof(m_cuts[0])), m_cuts.data());
            m_cutsDirty = false;
        }
        updates->updateDynamicBuffer(m_scene.get(), 0, sizeof(m_lens), &m_lens);
        updates->updateDynamicBuffer(m_transforms.get(), 0, quint32(m_count * sizeof(m_poses[0])), m_poses.data());
        const QRhiCommandBuffer::VertexInput inputs[]
            = { { m_quad.get(), 0 }, { m_templates.get(), 0 }, { m_transforms.get(), 0 } };
        for (size_t i = 0; i < m_targets.size(); ++i) {
            auto* target = m_targets[i]->target();
            const auto size = target->pixelSize();
            cb->beginPass(target, Qt::transparent, { 1, 0 }, i == 0 ? updates : nullptr);
            cb->setGraphicsPipeline(i ? m_blur.get() : m_chamber.get());
            cb->setViewport({ 0, 0, float(size.width()), float(size.height()) });
            cb->setShaderResources(m_bindings[i].get());
            cb->setVertexInput(0, i ? 1 : 3, inputs);
            cb->draw(4, i ? 1 : quint32(m_count));
            cb->endPass();
        }
        m_pending = false;
    }
    void render(const RenderState*) override { }
    RenderingFlags flags() const override
    {
        return NoExternalRendering;
    }
    void releaseResources() override
    {
        delete m_fold;
        m_fold = nullptr;
        m_chamber.reset();
        m_blur.reset();
        m_bindings = {};
        m_sampler.reset();
        m_quad.reset();
        m_templates.reset();
        m_transforms.reset();
        m_scene.reset();
        m_blurUniforms = {};
        m_targets = {};
        m_revision = 0;
        m_pending = false;
        m_initialize = true;
        m_blurState.invalidate();
    }

private:
    Buffer buffer(QRhiBuffer::Type type, QRhiBuffer::UsageFlags usage, quint32 size) const
    {
        Buffer b(m_rhi->newBuffer(type, usage, size));
        return b->create() ? std::move(b) : nullptr;
    }
    Pipeline pipeline(
        size_t pass, const QRhiVertexInputLayout& layout, const QShader& vert, const QShader& frag) const
    {
        if (!vert.isValid() || !frag.isValid())
            return {};
        Pipeline p(m_rhi->newGraphicsPipeline());
        p->setShaderStages({ { QRhiShaderStage::Vertex, vert }, { QRhiShaderStage::Fragment, frag } });
        p->setTopology(QRhiGraphicsPipeline::TriangleStrip);
        p->setVertexInputLayout(layout);
        p->setShaderResourceBindings(m_bindings[pass].get());
        p->setRenderPassDescriptor(m_targets[pass]->target()->renderPassDescriptor());
        if (!pass)
            p->setTargetBlends({ OffscreenTarget::screenBlend() });
        return p->create() ? std::move(p) : nullptr;
    }
    bool setup(QSize size)
    {
        m_rhi = m_window->rhi();
        if (!m_rhi || m_rhi->backend() == QRhi::Null || !m_rhi->isFeatureSupported(QRhi::Instancing))
            return false;
        for (auto& target : m_targets) {
            target = OffscreenTarget::create(m_window, m_rhi, size);
            if (!target)
                return false;
            target->texture()->setFiltering(QSGTexture::Linear);
            size = { std::max(1, size.width() / 2), std::max(1, size.height() / 2) };
        }
        m_quad = buffer(QRhiBuffer::Immutable, QRhiBuffer::VertexBuffer, 8 * sizeof(float));
        m_templates = buffer(QRhiBuffer::Static, QRhiBuffer::VertexBuffer, sizeof(m_cuts));
        m_transforms = buffer(QRhiBuffer::Dynamic, QRhiBuffer::VertexBuffer, sizeof(m_poses));
        m_scene = buffer(QRhiBuffer::Dynamic, QRhiBuffer::UniformBuffer, sizeof(m_lens) + 16);
        m_sampler.reset(m_rhi->newSampler(QRhiSampler::Linear, QRhiSampler::Linear, QRhiSampler::None,
            QRhiSampler::ClampToEdge, QRhiSampler::ClampToEdge));
        if (!m_quad || !m_templates || !m_transforms || !m_scene || !m_sampler->create())
            return false;
        using Resource = QRhiShaderResourceBinding;
        const auto stages = Resource::VertexStage | Resource::FragmentStage;
        for (size_t i = 0; i < m_bindings.size(); ++i) {
            m_bindings[i].reset(m_rhi->newShaderResourceBindings());
            if (!i)
                m_bindings[i]->setBindings({ Resource::uniformBuffer(0, stages, m_scene.get()) });
            else {
                m_blurUniforms[i - 1] = buffer(QRhiBuffer::Immutable, QRhiBuffer::UniformBuffer, 16);
                if (!m_blurUniforms[i - 1])
                    return false;
                m_bindings[i]->setBindings({ Resource::uniformBuffer(0, stages, m_blurUniforms[i - 1].get()),
                    Resource::sampledTexture(
                        1, Resource::FragmentStage, m_targets[i - 1]->texture()->rhiTexture(), m_sampler.get()) });
            }
            if (!m_bindings[i]->create())
                return false;
        }
        QRhiVertexInputLayout cut, quad;
        using Attr = QRhiVertexInputAttribute;
        cut.setBindings({ { 8 }, { sizeof(kaleidoscope_template), QRhiVertexInputBinding::PerInstance },
            { sizeof(kaleidoscope_pose), QRhiVertexInputBinding::PerInstance } });
        cut.setAttributes({ { 0, 0, Attr::Float2, 0 }, { 1, 1, Attr::Float4, 0 }, { 1, 2, Attr::Float4, 16 },
            { 1, 3, Attr::Float4, 32 }, { 1, 4, Attr::Float4, 48 }, { 1, 5, Attr::Float4, 64 },
            { 2, 6, Attr::Float4, 0 } });
        quad.setBindings({ { 8 } });
        quad.setAttributes({ { 0, 0, Attr::Float2, 0 } });
        static const auto cv = kaleidoscopeShader("chamber.vert"), cf = kaleidoscopeShader("chamber.frag"),
                          bv = kaleidoscopeShader("blur.vert"), bf = kaleidoscopeShader("blur.frag");
        m_chamber = pipeline(0, cut, cv, cf);
        m_blur = pipeline(1, quad, bv, bf);
        if (!m_chamber || !m_blur)
            return false;
        Bindings fold(m_rhi->newShaderResourceBindings());
        fold->setBindings({ Resource::uniformBuffer(0, stages, m_scene.get()),
            Resource::sampledTexture(
                1, Resource::FragmentStage, m_targets[0]->texture()->rhiTexture(), m_sampler.get()),
            Resource::sampledTexture(
                2, Resource::FragmentStage, m_targets[2]->texture()->rhiTexture(), m_sampler.get()) });
        return fold->create() && FoldNode::supports(m_window, m_rhi, fold.get());
    }
    QQuickWindow* m_window;
    QRhi* m_rhi = nullptr;
    FoldNode* m_fold = nullptr;
    QSizeF m_logical;
    std::array<std::unique_ptr<OffscreenTarget>, 3> m_targets;
    Buffer m_quad, m_templates, m_transforms, m_scene;
    std::array<Buffer, 2> m_blurUniforms;
    KaleidoscopeBlurUniforms m_blurState;
    std::unique_ptr<QRhiSampler> m_sampler;
    std::array<Bindings, 3> m_bindings;
    Pipeline m_chamber, m_blur;
    std::array<kaleidoscope_template, MaxPieces> m_cuts {};
    std::array<kaleidoscope_pose, MaxPieces> m_poses {};
    kaleidoscope_lens m_lens {};
    size_t m_count = 0;
    quint64 m_revision = 0, m_layout = 0;
    std::array<int, 4> m_lightKey {};
    std::array<kaleidoscope_pose, MaxPieces> m_previous {};
    bool m_initialize = true, m_pending = false, m_cutsDirty = false;
};
class KaleidoscopeItem final : public LivingVisualItem
{
public:
    using LivingVisualItem::LivingVisualItem;

protected:
    QSGNode* buildDepthNode(QSGNode* old, const VisualSampleLease& lease, const vd_sample_info& info,
        const vd_endpoint&, const vd_endpoint&) override
    {
        auto* node = old ? static_cast<ChamberNode*>(old) : new ChamberNode(window());
        if (node->sync(lease, info))
            return node;
        delete node;
        Q_EMIT gpuUnavailable();
        return nullptr;
    }
};
}
void registerDepthKaleidoscopeAdapter()
{
    Q_INIT_RESOURCE(depth_kaleidoscope_shaders);
    VisualDepthRegistry::registerItemFactory(5, [](QQuickItem* parent) { return new KaleidoscopeItem(parent); });
}

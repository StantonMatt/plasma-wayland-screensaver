// SPDX-License-Identifier: GPL-3.0-or-later
#include "livingvisualitem.h"
#include "dissolve.h"
#include "visualdepthregistry.h"
#include "visuals_depth_ribbons.h"
#include <QFile>
#include <QQuickWindow>
#include <QSGRenderNode>
#include <QSGTexture>
#include <rhi/qrhi.h>
#include <array>
#include <cstring>
#include <cmath>

static void initRibbonShaders()
{
    Q_INIT_RESOURCE(depth_ribbons_shaders);
}

namespace {
// Two adjacent RGBA32F texels hold the lower and upper head parameters.
constexpr int capsPerRow = 32;
constexpr int capRows = (VD_RIBBON_NODES + capsPerRow - 1) / capsPerRow;
constexpr std::array<float, 16> corners { 0, -1, 0, 1, 1, -1, 1, 1, 2, -1, 2, 1, 3, -1, 3, 1 };
struct Scene {
    float matrix[16], info[4];
    std::array<vd_ribbon_silk, 18> materials;
    std::array<std::array<float,4>,80> shoulders;
};
static_assert(sizeof(vd_ribbon_silk) == 64 && offsetof(Scene, materials) == 80);

QShader readShader(const QString& name)
{
    QFile file(QStringLiteral(":/ribbons/") + name + QStringLiteral(".qsb"));
    return file.open(QIODevice::ReadOnly) ? QShader::fromSerialized(file.readAll()) : QShader {};
}

class SilkNode final : public QSGRenderNode {
public:
    explicit SilkNode(QQuickWindow* window) : m_window(window) {}
    ~SilkNode() override { releaseResources(); }

    bool create(QSize size, bool fading)
    {
        auto* rhi = static_cast<QRhi*>(m_window->rendererInterface()->getResource(m_window, QSGRendererInterface::RhiResource));
        if (!rhi || rhi->backend() == QRhi::Null || !rhi->isFeatureSupported(QRhi::Instancing))
            return false;
        QRhiRenderTarget* mainTarget = nullptr;
        if (!fading) {
            auto* ri = m_window->rendererInterface();
            mainTarget = static_cast<QRhiRenderTarget*>(ri->getResource(m_window, QSGRendererInterface::RhiRedirectRenderTarget));
            if (!mainTarget) {
                auto* chain = static_cast<QRhiSwapChain*>(ri->getResource(m_window, QSGRendererInterface::RhiSwapchainResource));
                if (chain)
                    mainTarget = chain->currentFrameRenderTarget();
            }
            if (!mainTarget || !mainTarget->renderPassDescriptor())
                return false;
        }
        if (m_pipeline && m_rhi == rhi && size == m_size && fading == m_fading
            && (fading || (m_pass->isCompatible(mainTarget->renderPassDescriptor())
                           && m_pipeline->sampleCount() == mainTarget->sampleCount())))
            return true;
        releaseResources();
        m_rhi = rhi;
        m_size = size;
        static const auto vert = readShader(QStringLiteral("silk.vert"));
        static const auto frag = readShader(QStringLiteral("silk.frag"));
        if (!vert.isValid() || !frag.isValid())
            return false;
        m_fading = fading;
        if (fading) {
            std::unique_ptr<QRhiTexture> colour(rhi->newTexture(QRhiTexture::RGBA8, size, 1, QRhiTexture::RenderTarget));
            if (!colour->create())
                return false;
            // The QSG texture wrapper owns the RHI texture. Keep it alive until
            // after the render target on every failure path and on teardown.
            m_colour.reset(m_window->createTextureFromRhiTexture(colour.get(), QQuickWindow::TextureHasAlphaChannel));
            if (!m_colour)
                return false;
            auto* colourAttachment = colour.release();
            QRhiTextureRenderTargetDescription desc { QRhiColorAttachment(colourAttachment) };
            m_target.reset(rhi->newTextureRenderTarget(desc));
            m_pass.reset(m_target->newCompatibleRenderPassDescriptor());
            m_target->setRenderPassDescriptor(m_pass.get());
            if (!m_target->create())
                return false;
        } else {
            m_pass.reset(mainTarget->renderPassDescriptor()->newCompatibleRenderPassDescriptor());
        }
        m_ubo.reset(rhi->newBuffer(QRhiBuffer::Dynamic, QRhiBuffer::UniformBuffer, sizeof(Scene)));
        m_vertices.reset(rhi->newBuffer(QRhiBuffer::Dynamic, QRhiBuffer::VertexBuffer, sizeof(m_nodes)));
        m_capsTexture.reset(rhi->newTexture(QRhiTexture::RGBA32F, {2 * capsPerRow, capRows}));
        m_sampler.reset(rhi->newSampler(QRhiSampler::Nearest, QRhiSampler::Nearest, QRhiSampler::None,
                                       QRhiSampler::ClampToEdge, QRhiSampler::ClampToEdge));
        m_corners.reset(rhi->newBuffer(QRhiBuffer::Immutable, QRhiBuffer::VertexBuffer, sizeof(corners)));
        if (!m_ubo->create() || !m_vertices->create()
            || !m_corners->create() || !m_capsTexture->create() || !m_sampler->create())
            return false;
        m_bindings.reset(rhi->newShaderResourceBindings());
        constexpr auto vertex = QRhiShaderResourceBinding::VertexStage;
        m_bindings->setBindings({ QRhiShaderResourceBinding::uniformBuffer(0, vertex, m_ubo.get()),
            QRhiShaderResourceBinding::sampledTexture(1, vertex, m_capsTexture.get(), m_sampler.get()) });
        if (!m_bindings->create())
            return false;
        QRhiVertexInputLayout layout;
        layout.setBindings({ { 2 * sizeof(float) },
                             { sizeof(vd_ribbon_motion), QRhiVertexInputBinding::PerInstance },
                             { sizeof(vd_ribbon_motion), QRhiVertexInputBinding::PerInstance } });
        layout.setAttributes({
            { 0, 0, QRhiVertexInputAttribute::Float2, 0 },
            { 1, 1, QRhiVertexInputAttribute::Float4, 0 },  { 1, 2, QRhiVertexInputAttribute::Float4, 16 },
            { 1, 3, QRhiVertexInputAttribute::Float4, offsetof(vd_ribbon_motion, upper) }, { 1, 4, QRhiVertexInputAttribute::Float4, offsetof(vd_ribbon_motion, upper) + 16 },
            { 2, 5, QRhiVertexInputAttribute::Float4, 0 },  { 2, 6, QRhiVertexInputAttribute::Float4, 16 },
            { 2, 7, QRhiVertexInputAttribute::Float4, offsetof(vd_ribbon_motion, upper) }, { 2, 8, QRhiVertexInputAttribute::Float4, offsetof(vd_ribbon_motion, upper) + 16 },
            { 1, 9, QRhiVertexInputAttribute::UInt, offsetof(vd_ribbon_motion, tag) },
            { 2, 10, QRhiVertexInputAttribute::UInt, offsetof(vd_ribbon_motion, tag) }
        });
        m_pipeline.reset(rhi->newGraphicsPipeline());
        m_pipeline->setTopology(QRhiGraphicsPipeline::TriangleStrip);
        m_pipeline->setShaderStages({ { QRhiShaderStage::Vertex, vert }, { QRhiShaderStage::Fragment, frag } });
        m_pipeline->setVertexInputLayout(layout);
        m_pipeline->setShaderResourceBindings(m_bindings.get());
        m_pipeline->setRenderPassDescriptor(m_pass.get());
        QRhiGraphicsPipeline::TargetBlend blend;
        blend.enable = true;
        // Uniform peak attenuation keeps stacked light coloured. It has the
        // same fixed-function cost as ordinary alpha blending and needs no
        // floating-point target or extra resolve pass.
        // Retain the still's opacity for a dissolve over a non-black background.
        blend.srcColor = blend.srcAlpha = QRhiGraphicsPipeline::One;
        blend.dstColor = blend.dstAlpha = QRhiGraphicsPipeline::OneMinusSrcAlpha;
        if (!fading) {
            m_pipeline->setSampleCount(mainTarget->sampleCount());
            m_pipeline->setFlags(QRhiGraphicsPipeline::UsesScissor);
            blend.srcAlpha = QRhiGraphicsPipeline::Zero;
            blend.dstAlpha = QRhiGraphicsPipeline::One;
        }
        m_pipeline->setTargetBlends({ blend });
        if (!m_pipeline->create())
            return false;
        m_uploadCorners = true;
        return true;
    }
    // Only reduced motion needs two targets, and only during its eight-second
    // dissolve. Blend the finished RGBA8 stills so saturated overlaps fade at
    // precisely the same rate as every other pixel.
    bool prepareDissolve(QSizeF logical)
    {
        if (m_dissolve)
            return true;
        std::unique_ptr<QRhiTexture> texture(m_rhi->newTexture(QRhiTexture::RGBA8, m_size, 1, QRhiTexture::RenderTarget));
        if (!texture->create())
            return false;
        m_nextColour.reset(m_window->createTextureFromRhiTexture(texture.get(), QQuickWindow::TextureHasAlphaChannel));
        if (!m_nextColour)
            return false;
        QRhiTextureRenderTargetDescription desc { QRhiColorAttachment(texture.release()) };
        m_nextTarget.reset(m_rhi->newTextureRenderTarget(desc));
        m_nextTarget->setRenderPassDescriptor(m_pass.get());
        if (!m_nextTarget->create())
            return false;
        m_dissolve = new Ribbons::DissolveNode(m_colour.get(), m_nextColour.get(), logical);
        return true;
    }
    bool sync(const VisualSampleLease& lease, const vd_sample_info& info, float mix)
    {
        const auto& view = info.view;
        const float alpha = mix < 0.f ? float(info.alpha) : 0.f;
        if (visuals_ribbons_same_pair(m_lastLease.get(), lease.get())) {
            if (m_fading && m_dissolve) {
                m_dissolve->setFraction(mix);
                return true;
            }
            if (mix == m_lastMix) {
                if (m_scene.info[2] != alpha) {
                    // Only the presentation fraction changes between 30 Hz
                    // knots. Keep the culled endpoint pairs on the GPU.
                    m_scene.info[2] = alpha;
                    m_pending = true;
                    markDirty(DirtyForceUpdate);
                }
                return true;
            }
        }
        m_lastMix = mix;
        m_lastLease = lease;
        m_uploadRecords = true;
        m_scene.info[2] = alpha;
        const size_t count = visuals_ribbons_write_packed(lease.get(), m_nodes.data(), m_nodes.size(),
            m_caps.data(), VD_RIBBON_NODES, &m_capCount, m_draws.data(),
            m_draws.size(), &m_used);
        m_rect = QRectF(0, 0, view.viewport.width, view.viewport.height);
        if (m_fading) {
            if (!prepareDissolve(m_rect.size()))
                return false;
            if (!m_dissolve->parent())
                appendChildNode(m_dissolve);
            m_dissolve->setFraction(mix);
        }
        m_transform.setToIdentity();
        m_transform.translate(float(view.offset_x), float(view.offset_y));
        m_transform.scale(float(view.uniform_scale));
        m_transform.translate(float(-view.viewport.x), float(-view.viewport.y));
        m_scene.info[0] = float(view.uniform_scale * view.dpr);
        m_batches.fill({});
        for (size_t i = 0; i < count; ++i) {
            const auto& range = m_draws[i];
            // The layers are contiguous and each ribbon ends with a shader break.
            // Submit them in their original order without rebinding the stream.
            const size_t stream = range.motes ? 1 : 0;
            auto& batch = m_batches[m_fading ? range.composition : 0][stream];
            if (!batch.count)
                batch.first = range.first;
            batch.count = range.first + range.count - batch.first;
            m_scene.materials[range.material] = range.silk;
            if (!range.motes) {
                m_scene.shoulders[range.strand] = { range.shoulder_gain[0], range.shoulder_gain[1], 0.f, 0.f };

            }
        }
        m_pending = true;
        markDirty(DirtyForceUpdate);
        return true;
    }
    void prepare() override
    {
        auto* cb = commandBuffer();
        if (!cb || !m_pipeline)
            return;
        QMatrix4x4 projection;
        if (m_fading) {
            if (!m_pending)
                return;
            if (m_rhi->isYUpInFramebuffer())
                projection.scale(1, -1);
            projection.ortho(0, float(m_rect.width()), float(m_rect.height()), 0, -1, 1);
            projection = m_rhi->clipSpaceCorrMatrix() * projection * m_transform;
        } else {
            projection = *projectionMatrix() * *matrix() * m_transform;
        }
        const float opacity = m_fading ? 1.f : float(inheritedOpacity());
        const bool transformed = std::memcmp(m_scene.matrix, projection.constData(), sizeof(m_scene.matrix)) != 0;
        if (!m_pending && !transformed && opacity == m_scene.info[1])
            return;
        std::memcpy(m_scene.matrix, projection.constData(), sizeof(m_scene.matrix));
        m_scene.info[1] = opacity;
        m_pending = false;
        auto* updates = m_rhi->nextResourceUpdateBatch();
        if (m_uploadCorners) {
            updates->uploadStaticBuffer(m_corners.get(), corners.data());
            m_uploadCorners = false;
        }
        if (m_uploadRecords) {
            if (m_used)
                updates->updateDynamicBuffer(m_vertices.get(), 0, m_used * sizeof(vd_ribbon_motion), m_nodes.data());
            if (m_capCount) {
                const int rows = int((m_capCount + capsPerRow - 1) / capsPerRow);
                QRhiTextureSubresourceUploadDescription data(m_caps.data(), rows * capsPerRow * sizeof(vd_ribbon_cap));
                data.setSourceSize({ 2 * capsPerRow, rows });
                updates->uploadTexture(m_capsTexture.get(), QRhiTextureUploadEntry(0, 0, data));
            }
            updates->updateDynamicBuffer(m_ubo.get(), 0, sizeof(Scene), &m_scene);
            m_uploadRecords = false;
        } else if (transformed) {
            updates->updateDynamicBuffer(m_ubo.get(), 0, offsetof(Scene, materials), &m_scene);
        } else {
            updates->updateDynamicBuffer(m_ubo.get(), offsetof(Scene, info), sizeof(m_scene.info), m_scene.info);
        }
        if (m_fading) {
            cb->beginPass(m_target.get(), Qt::transparent, { 1, 0 }, updates);
            draw(cb, m_size, 0);
            cb->endPass();
            cb->beginPass(m_nextTarget.get(), Qt::transparent, { 1, 0 });
            draw(cb, m_size, 1);
            cb->endPass();
        } else {
            cb->resourceUpdate(updates);
        }
    }
    void draw(QRhiCommandBuffer* cb, QSize size, size_t composition, const QRect* scissor = nullptr)
    {
        cb->setGraphicsPipeline(m_pipeline.get());
        cb->setViewport({ 0, 0, float(size.width()), float(size.height()) });
        if (scissor)
            cb->setScissor({ scissor->x(), scissor->y(), scissor->width(), scissor->height() });
        cb->setShaderResources(m_bindings.get());
        const QRhiCommandBuffer::VertexInput cornersInput { m_corners.get(), 0 };
        cb->setVertexInput(0, 1, &cornersInput);
        const auto& [silk, sparks] = m_batches[composition];
        if (silk.count > 1)
            drawStream(cb, silk.first, 8, silk.count - 1);
        if (sparks.count)
            drawStream(cb, sparks.first, 4, sparks.count);
    }
    void drawStream(QRhiCommandBuffer* cb, size_t first, quint32 vertices, quint32 instances)
    {
        const quint32 offset = first * sizeof(vd_ribbon_motion);
        const QRhiCommandBuffer::VertexInput inputs[] = {
            { m_vertices.get(), offset }, { m_vertices.get(), offset + sizeof(vd_ribbon_motion) }
        };
        cb->setVertexInput(1, 2, inputs);
        cb->draw(vertices, instances);
    }
    void render(const RenderState* state) override
    {
        if (!m_fading && m_pipeline && commandBuffer()) {
            const QSize size = renderTarget()->pixelSize();
            const QRect clip = state->scissorEnabled() ? state->scissorRect() : QRect(QPoint(), size);
            draw(commandBuffer(), size, 0, &clip);
        }
    }
    RenderingFlags flags() const override { return NoExternalRendering | BoundedRectRendering; }
    StateFlags changedStates() const override { return ViewportState | ScissorState | BlendState; }
    QRectF rect() const override { return m_rect; }
    void releaseResources() override
    {
        m_pipeline.reset();
        m_bindings.reset();
        m_capsTexture.reset();
        m_sampler.reset();
        m_ubo.reset();
        m_lastLease.reset();
        m_vertices.reset();
        m_corners.reset();
        m_nextTarget.reset();
        m_target.reset();
        m_pass.reset();
        delete m_dissolve;
        m_dissolve = nullptr;
        m_nextColour.reset();
        m_colour.reset();
        m_rhi = nullptr;
    }
private:
    QQuickWindow* m_window;
    QRhi* m_rhi = nullptr;
    QSize m_size;
    size_t m_used = 0;
    bool m_fading = false;
    float m_lastMix = -2.f;
    bool m_uploadRecords = true;
    Ribbons::DissolveNode* m_dissolve = nullptr;
    bool m_pending = false;
    std::array<vd_ribbon_motion, VD_RIBBON_NODES> m_nodes;
    std::array<vd_ribbon_cap, capRows * capsPerRow> m_caps {};
    size_t m_capCount = 0;
    std::unique_ptr<QRhiTexture> m_capsTexture;
    std::unique_ptr<QRhiSampler> m_sampler;
    std::array<vd_ribbon_draw, VD_RIBBON_DRAWS> m_draws;
    Scene m_scene {};
    struct Batch {
        quint32 first = 0, count = 0;
    };
    std::array<std::array<Batch, 2>, 2> m_batches {};
    bool m_uploadCorners = false;
    VisualSampleLease m_lastLease;
    QMatrix4x4 m_transform;
    QRectF m_rect;
    std::unique_ptr<QSGTexture> m_colour, m_nextColour;
    std::unique_ptr<QRhiTextureRenderTarget> m_target, m_nextTarget;
    std::unique_ptr<QRhiRenderPassDescriptor> m_pass;
    std::unique_ptr<QRhiBuffer> m_ubo, m_vertices, m_corners;
    std::unique_ptr<QRhiShaderResourceBindings> m_bindings;
    std::unique_ptr<QRhiGraphicsPipeline> m_pipeline;
};

class RibbonItem final : public LivingVisualItem {
public:
    using LivingVisualItem::LivingVisualItem;
protected:
    QSGNode* buildDepthNode(QSGNode* old, const VisualSampleLease& lease, const vd_sample_info& info,
        const vd_endpoint&, const vd_endpoint&) override
    {
        auto* node = old ? static_cast<SilkNode*>(old) : new SilkNode(window());
        const auto& v = info.view;
        const float mix = visuals_ribbons_dissolve(lease.get());
        if (!node->create(QSize(qRound(v.viewport.width * v.dpr), qRound(v.viewport.height * v.dpr)), mix > 0.f && mix < 1.f)
            || !node->sync(lease, info, mix)) {
            delete node;
            Q_EMIT gpuUnavailable();
            return nullptr;
        }
        return node;
    }
};
}
void registerDepthRibbonsAdapter()
{
    initRibbonShaders();
    VisualDepthRegistry::registerItemFactory(4, [](QQuickItem* parent) { return new RibbonItem(parent); });
}

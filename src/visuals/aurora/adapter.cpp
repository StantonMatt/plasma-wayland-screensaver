// SPDX-License-Identifier: GPL-3.0-or-later
#include "livingvisualitem.h"
#include "shore.h"
#include "visualdepthregistry.h"
#include "visuals/common/offscreentarget.h"
#include "visuals_depth_aurora.h"
#include <QFile>
#include <QSGRenderNode>
#include <QSGOpacityNode>
#include <rhi/qshader.h>
#include <array>
#include <bit>
#include <cmath>
#include <cstring>

namespace {
QShader shader(const QString& name)
{
    QFile file(QStringLiteral(":/depth/aurora/curtain.") + name + QStringLiteral(".qsb"));
    return file.open(QIODevice::ReadOnly) ? QShader::fromSerialized(file.readAll()) : QShader();
}

struct Layer {
    QByteArray records;
    std::unique_ptr<QRhiBuffer> buffer;
    size_t capacity = 0;
    size_t count = 0;
    bool dirty = false;
    bool sync(QRhi* rhi, const VisualSampleLease& lease, unsigned layer, int side)
    {
        size_t required = capacity;
        if (!required)
            required = 6 * visuals_aurora_quad_capacity(lease.get(), layer, side);
        if (!required) {
            count = 0;
            return true;
        }
        do {
            if (required > capacity) {
                capacity = std::bit_ceil(required);
                const auto bytes = quint32(capacity * sizeof(aurora_vertex));
                buffer.reset(rhi->newBuffer(layer == 1 ? QRhiBuffer::Dynamic : QRhiBuffer::Static,
                    QRhiBuffer::VertexBuffer, bytes));
                if (!buffer->create())
                    return false;
            }
            // During scene-graph synchronization the active frame can accept
            // the strip directly into its dynamic vertex buffer. Backends
            // without a mapping use the usual resource-update batch.
            char* mapped = layer == 1 && rhi->isRecordingFrame()
                ? buffer->beginFullDynamicBufferUpdateForCurrentFrame() : nullptr;
            if (mapped) {
                required = visuals_aurora_strip(lease.get(), layer, side,
                    reinterpret_cast<aurora_vertex*>(mapped), capacity);
                buffer->endFullDynamicBufferUpdateForCurrentFrame();
                if (required <= capacity) {
                    count = required;
                    dirty = false;
                    return true;
                }
            } else {
                records.resizeForOverwrite(qsizetype(capacity * sizeof(aurora_vertex)));
                required = visuals_aurora_strip(lease.get(), layer, side,
                    reinterpret_cast<aurora_vertex*>(records.data()), capacity);
            }
        } while (required > capacity);
        count = required;
        records.resize(qsizetype(count * sizeof(aurora_vertex)));
        dirty = count > 0;
        return true;
    }
    void upload(QRhiResourceUpdateBatch* updates)
    {
        if (!dirty)
            return;
#if QT_VERSION >= QT_VERSION_CHECK(6, 10, 0)
        // The batch can share storage until submit on newer RHI backends.
        if (buffer->type() == QRhiBuffer::Dynamic)
            updates->updateDynamicBuffer(buffer.get(), 0, records);
        else
            updates->uploadStaticBuffer(buffer.get(), 0, records);
#else
        if (buffer->type() == QRhiBuffer::Dynamic)
            updates->updateDynamicBuffer(buffer.get(), 0, quint32(records.size()), records.constData());
        else
            updates->uploadStaticBuffer(buffer.get(), 0, quint32(records.size()), records.constData());
#endif
        dirty = false;
    }
};

class AuroraNode final : public QSGRenderNode {
public:
    explicit AuroraNode(QQuickWindow* window)
        : m_window(window)
    {
    }
    ~AuroraNode() override { releaseResources(); }
    bool sync(const VisualSampleLease& lease, const vd_sample_info& info, const vd_endpoint& endpoint, int side)
    {
        const float scale = visuals_aurora_atmosphere(lease.get(), side, &m_atmosphere);
        if (scale <= 0)
            return false;
        const auto& view = info.view;
        const QSizeF logical(view.viewport.width, view.viewport.height);
        // Fine optical detail is reconstructed by the linear composite. The
        // target accounts for both curtain coverage and fractional DPR.
        m_guard = (3 * std::max(m_atmosphere.optics[3], 16.3f * m_atmosphere.bounds[3] / 1440.f) + 2)
            * m_atmosphere.view[2];
        // Keep the optical margin an integral number of physical texels. A
        // fractional margin resamples the entire emission target at composition.
        const qreal pixelScale = view.dpr * scale;
        m_guard = std::ceil(m_guard * pixelScale) / pixelScale;
        const QSize pixels(std::max(1, qRound((logical.width() + 2 * m_guard) * view.dpr * scale)),
            std::max(1, qRound(logical.height() * view.dpr * scale)));
        const quint64 pose = side == 0 ? info.lower_tick : info.upper_tick;
        const bool changed = !m_target || m_size != pixels || m_logical != logical
            || m_generation != info.generations.layout || (side >= 0 && m_pose != pose);
        if (side >= 0 && !changed)
            return true;
        if (changed) {
            releaseResources();
            m_rhi = m_window->rhi();
            if (!m_rhi || m_rhi->backend() == QRhi::Null)
                return false;
            // Avoid quantisation hatch when many weak sheets screen together.
            // The star and terrain paths retain their existing formats.
            m_target = OffscreenTarget::create(m_window, m_rhi, pixels, {}, QRhiTexture::RGBA16F);
            m_stars = OffscreenTarget::create(m_window, m_rhi, pixels);
            if (!m_target || !m_stars || !setup(endpoint, lease, side))
                return false;
            m_size = pixels;
            m_generation = info.generations.layout;
            m_pose = pose;
            qInfo("Visuals Aurora: living curtain screen accumulator %dx%d", pixels.width(), pixels.height());
            if (!m_layers[0].sync(m_rhi, lease, 0, side))
                return false;
            m_target->texture()->setFiltering(QSGTexture::Linear);
            m_stars->texture()->setFiltering(QSGTexture::Linear);
            m_display = new AuroraShore(m_target->texture(), m_stars->texture(), m_horizon.get(),
                m_landWidth, m_atmosphere, logical, m_guard);
            appendChildNode(m_display);
        }
        // The native composite also consumes live time and optical envelopes.
        // Updating only the offscreen camera freezes its dither and aperture.
        if (!changed)
            m_display->sync(m_atmosphere);
        m_logical = logical;
        m_pending = true;
        markDirty(DirtyForceUpdate);
        return m_layers[1].sync(m_rhi, lease, 1, side);
    }

    void prepare() override
    {
        auto* cb = commandBuffer();
        if (!cb || !m_target || !m_pending)
            return;
        auto* updates = m_rhi->nextResourceUpdateBatch();
        if (!m_fieldBytes.isEmpty()) {
            updates->uploadTexture(m_field.get(),
                QRhiTextureUploadDescription(
                    { QRhiTextureUploadEntry(0, 0, QRhiTextureSubresourceUploadDescription(m_fieldBytes)) }));
            updates->uploadTexture(m_horizon->rhiTexture(),
                QRhiTextureUploadDescription(
                    { QRhiTextureUploadEntry(0, 0, QRhiTextureSubresourceUploadDescription(m_landBytes)) }));
            m_landBytes.clear();
            m_fieldBytes.clear();

        }
        for (auto& layer : m_layers)
            layer.upload(updates);
        QMatrix4x4 matrix;
        if (m_rhi->isYUpInFramebuffer())
            matrix.scale(1, -1);
        matrix.ortho(-m_guard, float(m_logical.width()) + m_guard, float(m_logical.height()), 0, -1, 1);
        matrix = m_rhi->clipSpaceCorrMatrix() * matrix;
        updates->updateDynamicBuffer(m_uniforms.get(), 0, 64, matrix.constData());
        updates->updateDynamicBuffer(m_uniforms.get(), 64, sizeof(m_atmosphere), &m_atmosphere);
        // Filter the extended sheets independently of point sources. Otherwise a
        // star can change the reconstruction weights of brighter nearby rays.
        const auto draw = [&](OffscreenTarget& target, QRhiGraphicsPipeline* pipeline,
                              const Layer& layer, QRhiResourceUpdateBatch* changes) {
            cb->beginPass(target.target(), Qt::transparent, { 1, 0 }, changes);
            if (layer.count) {
                cb->setGraphicsPipeline(pipeline);
                cb->setViewport({ 0, 0, float(m_size.width()), float(m_size.height()) });
                cb->setShaderResources(m_bindings.get());
                const QRhiCommandBuffer::VertexInput input { layer.buffer.get(), 0 };
                cb->setVertexInput(0, 1, &input);
                cb->draw(quint32(layer.count));
            }
            cb->endPass();
        };
        draw(*m_stars, m_starsScreen.get(), m_layers[0], updates);
        draw(*m_target, m_screen.get(), m_layers[1], nullptr);
        m_pending = false;
    }
    void render(const RenderState*) override { }
    RenderingFlags flags() const override { return NoExternalRendering; }
    void releaseResources() override
    {
        delete m_display;
        m_display = nullptr;
        m_screen.reset();
        m_starsScreen.reset();
        m_horizon.reset();
        m_landBytes.clear();
        m_bindings.reset();
        m_sampler.reset();
        m_field.reset();
        m_fieldBytes.clear();
        m_uniforms.reset();
        for (auto& layer : m_layers)
            layer = Layer {};
        m_target.reset();
        m_stars.reset();
        m_size = {};
        m_generation = 0;
        m_pending = false;
    }

private:
    bool setup(const vd_endpoint& ep, const VisualSampleLease& lease, int side)
    {
        if (!ep.texture_bytes || !ep.texture_width || !ep.texture_height)
            return false;
        m_fieldBytes = QByteArray(reinterpret_cast<const char*>(ep.texture_bytes), qsizetype(ep.texture_size));
        m_field.reset(m_rhi->newTexture(QRhiTexture::RGBA8, { int(ep.texture_width), int(ep.texture_height) }));
        m_sampler.reset(m_rhi->newSampler(QRhiSampler::Linear, QRhiSampler::Linear, QRhiSampler::None,
            QRhiSampler::Repeat, QRhiSampler::ClampToEdge));
        m_uniforms.reset(
            m_rhi->newBuffer(QRhiBuffer::Dynamic, QRhiBuffer::UniformBuffer, 64 + sizeof(m_atmosphere)));
        if (!m_field->create() || !m_sampler->create() || !m_uniforms->create())
            return false;
        m_bindings.reset(m_rhi->newShaderResourceBindings());
        m_bindings->setBindings(
            { QRhiShaderResourceBinding::uniformBuffer(0,
                  QRhiShaderResourceBinding::VertexStage | QRhiShaderResourceBinding::FragmentStage,
                  m_uniforms.get()),
                QRhiShaderResourceBinding::sampledTexture(
                    1, QRhiShaderResourceBinding::FragmentStage, m_field.get(), m_sampler.get()) });
        if (!m_bindings->create())
            return false;
        static const QShader vertex = shader(QStringLiteral("vert")), fragment = shader(QStringLiteral("frag"));
        if (!vertex.isValid() || !fragment.isValid())
            return false;
        QRhiVertexInputLayout layout;
        layout.setBindings({ { sizeof(aurora_vertex) } });
        layout.setAttributes({ { 0, 0, QRhiVertexInputAttribute::Float4, 0 },
            { 0, 1, QRhiVertexInputAttribute::Float4, 16 }, { 0, 2, QRhiVertexInputAttribute::Float4, 32 } });
        const auto createPipeline = [&](std::unique_ptr<QRhiGraphicsPipeline>& pipeline, OffscreenTarget& target) {
            pipeline.reset(m_rhi->newGraphicsPipeline());
            pipeline->setShaderStages(
                { { QRhiShaderStage::Vertex, vertex }, { QRhiShaderStage::Fragment, fragment } });
            pipeline->setTopology(QRhiGraphicsPipeline::TriangleStrip);
            pipeline->setVertexInputLayout(layout);
            pipeline->setShaderResourceBindings(m_bindings.get());
            pipeline->setRenderPassDescriptor(target.target()->renderPassDescriptor());
            pipeline->setTargetBlends({ OffscreenTarget::screenBlend() });
            return pipeline->create();
        };
        if (!createPipeline(m_screen, *m_target) || !createPipeline(m_starsScreen, *m_stars))
            return false;
        const uint8_t* land = visuals_aurora_horizon(lease.get(), side, &m_landWidth);
        if (!land || !m_landWidth)
            return false;
        m_landBytes = QByteArray(reinterpret_cast<const char*>(land), qsizetype(m_landWidth) * 8);
        std::unique_ptr<QRhiTexture> skyline(m_rhi->newTexture(QRhiTexture::RGBA8, { int(m_landWidth), 2 }));
        if (!skyline->create())
            return false;
        m_horizon.reset(m_window->createTextureFromRhiTexture(skyline.get()));
        if (!m_horizon)
            return false;
        skyline.release();
        m_horizon->setFiltering(QSGTexture::Linear);
        return AuroraShore::supports(m_window, m_rhi, m_uniforms.get(),
            m_target->texture()->rhiTexture(), m_stars->texture()->rhiTexture(), m_horizon->rhiTexture(), m_sampler.get());
    }
    QQuickWindow* m_window;
    QRhi* m_rhi = nullptr;
    QSize m_size;
    QSizeF m_logical;
    float m_guard = 0;
    quint64 m_generation = 0, m_pose = 0;
    bool m_pending = false;
    aurora_atmosphere m_atmosphere {};
    std::unique_ptr<OffscreenTarget> m_target, m_stars;
    std::unique_ptr<QRhiBuffer> m_uniforms;
    std::unique_ptr<QRhiTexture> m_field;
    std::unique_ptr<QRhiSampler> m_sampler;
    std::unique_ptr<QRhiShaderResourceBindings> m_bindings;
    std::unique_ptr<QRhiGraphicsPipeline> m_screen, m_starsScreen;
    QByteArray m_fieldBytes;
    std::array<Layer, 2> m_layers;
    uint32_t m_landWidth = 0;
    QByteArray m_landBytes;
    std::unique_ptr<QSGTexture> m_horizon;
    AuroraShore* m_display = nullptr;
};

class AuroraScene final : public QSGNode {
public:
    explicit AuroraScene(QQuickWindow* window)
        : base(new AuroraNode(window))
    {
        appendChildNode(base);
    }
    bool sync(QQuickWindow* window, const VisualSampleLease& lease, const vd_sample_info& info,
        const vd_endpoint& lower, const vd_endpoint& upper)
    {
        const float fade = visuals_aurora_dissolve(lease.get());
        // Negative requests live interpolation. A still selects just one endpoint.
        const int side = fade < 0 ? -1 : int(fade >= 1);
        if (!base->sync(lease, info, side == 0 ? lower : upper, side))
            return false;
        if (fade > 0 && fade < 1) {
            if (!opacity) {
                opacity = new QSGOpacityNode;
                incoming = new AuroraNode(window);
                opacity->appendChildNode(incoming);
                appendChildNode(opacity);
            }
            if (!incoming->sync(lease, info, upper, 1))
                return false;
            opacity->setOpacity(fade);
        } else if (opacity) {
            delete opacity;
            opacity = nullptr;
            incoming = nullptr;
        }
        return true;
    }

private:
    AuroraNode* base;
    AuroraNode* incoming = nullptr;
    QSGOpacityNode* opacity = nullptr;
};

class AuroraItem final : public LivingVisualItem {
public:
    using LivingVisualItem::LivingVisualItem;

protected:
    QSGNode* buildDepthNode(QSGNode* old, const VisualSampleLease& lease, const vd_sample_info& info,
        const vd_endpoint& lower, const vd_endpoint& upper) override
    {
        auto* node = old ? static_cast<AuroraScene*>(old) : new AuroraScene(window());
        if (!node->sync(window(), lease, info, lower, upper)) {
            delete node;
            Q_EMIT gpuUnavailable();
            return nullptr;
        }
        return node;
    }
};
}

void registerDepthAuroraAdapter()
{
    Q_INIT_RESOURCE(depth_aurora_shaders);
    VisualDepthRegistry::registerItemFactory(1, [](QQuickItem* parent) { return new AuroraItem(parent); });
}

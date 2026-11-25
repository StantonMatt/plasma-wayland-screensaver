// SPDX-License-Identifier: GPL-3.0-or-later
#include "bolts.h"
#include "material.h"
#include "../common/offscreentarget.h"
#include <QQuickWindow>
#include <QSGSimpleTextureNode>
#include <algorithm>
#include <bit>
#include <cmath>
#include <cstring>

struct RainBolts::State {
    QQuickWindow* window;
    QSGTexture* atlas;
    QRhi* rhi = nullptr;
    std::unique_ptr<OffscreenTarget> target;
    std::unique_ptr<QRhiBuffer> uniforms, vertices;
    std::unique_ptr<QRhiSampler> sampler;
    std::unique_ptr<QRhiShaderResourceBindings> bindings;
    std::unique_ptr<QRhiGraphicsPipeline> pipeline;
    std::unique_ptr<VisualDataTexture> records;
    QSGSimpleTextureNode* display = nullptr;
    QSize pixels;
    QRectF rect;
    QSizeF viewport;
    qreal dpr = 0.;
    vd_rain_uniforms data {};
    QByteArray vertexBytes;
    QByteArray recordBytes;
    uint64_t revision = UINT64_MAX;
    size_t capacity = 0, count = 0;
    bool dirty = false;
};
RainBolts::RainBolts(QQuickWindow* window, QSGTexture* atlas)
    : d(std::make_unique<State>()) { d->window = window; d->atlas = atlas; }
RainBolts::~RainBolts() { releaseResources(); }
void RainBolts::releaseResources()
{
    delete d->display;
    d->display = nullptr;
    d->pipeline.reset(); d->bindings.reset(); d->sampler.reset();
    d->vertices.reset(); d->uniforms.reset(); d->records.reset(); d->target.reset();
    d->rhi = nullptr; d->capacity = d->count = 0; d->revision = UINT64_MAX;
    d->recordBytes.clear();
}
void RainBolts::sync(std::span<const vd_rain_head> heads, uint64_t revision, const vd_rain_uniforms& u)
{
#ifdef PVS_ENABLE_VISUAL_CAPTURE
    // Native pixel tests need the same rain, flash, masks and simulation with
    // only channel emission absent. This hook is confined to the private
    // capture harness; it is absent from installed production builds.
    static const bool reference = qEnvironmentVariableIntValue("PVS_MATRIX_BOLT_REFERENCE") == 1
        && qEnvironmentVariableIsSet("PVS_PRIVATE_HARNESS_ROOT")
        && qEnvironmentVariableIsSet("PVS_VISUAL_CAPTURE_SCRIPT");
#else
    constexpr bool reference = false;
#endif
    if (heads.empty() || reference) {
        if (d->target) releaseResources();
        return;
    }
    const qreal scale = d->window->effectiveDevicePixelRatio();
    const QSizeF viewport(u.viewport[2], u.viewport[3]);
    const bool sameView = d->viewport == viewport && d->dpr == scale
        && d->data.viewport[0] == u.viewport[0] && d->data.viewport[1] == u.viewport[1];
    if (d->target && sameView && d->count == heads.size()
        && (revision == d->revision || (d->recordBytes.size() == qsizetype(heads.size_bytes())
            && std::memcmp(d->recordBytes.constData(), heads.data(), heads.size_bytes()) == 0))) {
        d->data = u;
        d->data.light[3] = 1.f;
        d->data.grid[3] = float(d->count);
        d->revision = revision;
        markDirty(DirtyForceUpdate);
        return;
    }
    QRectF bounds;
    for (const auto& h : heads) {
        const float pad = 4.f * 0.55f * h.ink[0] + 2.f;
        const auto x = std::min(h.path[0], h.path[2]) - u.viewport[0] - pad;
        const auto y = std::min(h.path[1], h.path[3]) - u.viewport[1] - pad;
        bounds |= QRectF(x, y, std::abs(h.path[2] - h.path[0]) + 2*pad,
            std::abs(h.path[3] - h.path[1]) + 2*pad);
    }
    bounds &= QRectF(0, 0, u.viewport[2], u.viewport[3]);
    if (bounds.isEmpty()) { releaseResources(); return; }
    const QRect physical = QRectF(bounds.topLeft()*scale, bounds.size()*scale).toAlignedRect();
    const QRectF rect(QPointF(physical.topLeft())/scale, QSizeF(physical.size())/scale);
    auto* rhi = d->window->rhi();
    if (!rhi) return;
    if (!d->target || d->rhi != rhi || d->pixels != physical.size()) {
        releaseResources();
        d->rhi = rhi;
        d->pixels = physical.size();
        d->target = OffscreenTarget::create(d->window, rhi, d->pixels,
            QQuickWindow::TextureHasAlphaChannel, QRhiTexture::RGBA16F);
        if (!d->target) return;
        d->uniforms.reset(rhi->newBuffer(QRhiBuffer::Dynamic, QRhiBuffer::UniformBuffer, 64 + sizeof(u)));
        d->sampler.reset(rhi->newSampler(QRhiSampler::Nearest, QRhiSampler::Nearest, QRhiSampler::None,
            QRhiSampler::ClampToEdge, QRhiSampler::ClampToEdge));
        if (!d->uniforms->create() || !d->sampler->create()) { releaseResources(); return; }
        d->records = std::make_unique<VisualDataTexture>();
        d->display = new QSGSimpleTextureNode;
        // Every union pixel has alpha zero. Standard premultiplied blending
        // therefore adds RGB onto the scene; MAX never sees sky or rain.
        d->display->setTexture(d->target->texture());
        d->display->setFiltering(QSGTexture::Linear);
        appendChildNode(d->display);
    }
    d->rect = rect;
    d->display->setRect(rect);
    d->data = u;
    d->data.light[3] = 1.f; // Scene opacity belongs to the display node.
    d->data.grid[3] = float(heads.size());
    d->dpr = scale;
    if (d->revision != revision || d->viewport != viewport) {
        d->revision = revision;
        d->viewport = viewport;
        const auto capacity = std::bit_ceil(std::max(size_t(1), heads.size()));
        if (d->capacity != capacity) {
            d->pipeline.reset(); d->bindings.reset();
            d->vertices.reset(rhi->newBuffer(QRhiBuffer::Dynamic, QRhiBuffer::VertexBuffer,
                int(capacity*6*4*sizeof(float))));
            if (!d->vertices->create()) { releaseResources(); return; }
            d->capacity = capacity;
        }
        d->count = heads.size();
        d->vertexBytes.resize(qsizetype(d->count*6*4*sizeof(float)));
        auto* out = reinterpret_cast<float*>(d->vertexBytes.data());
        constexpr int corners[][2] = {{0,0},{1,0},{0,1},{0,1},{1,0},{1,1}};
        for (size_t i=0; i<d->count; ++i) for (auto& c : corners) {
            *out++ = float(viewport.width())*c[0]; *out++ = float(viewport.height())*c[1];
            *out++ = float(i); *out++ = 0.f;
        }
        d->recordBytes = QByteArray(reinterpret_cast<const char*>(heads.data()), qsizetype(heads.size_bytes()));
        d->records->setData(d->recordBytes, {3, int(capacity)}, {3, int(d->count)});
#ifdef PVS_ENABLE_VISUAL_CAPTURE
        qInfo("Matrix bolt union cached: %zu segments, %dx%d target, birth %.0f, view %.1f,%.1f",
            d->count, d->pixels.width(), d->pixels.height(), heads[0].light[2], u.viewport[0], u.viewport[1]);
        for (size_t i=0; i<heads.size(); ++i) {
            const auto& h = heads[i];
            if (h.light[3] > 0 && (i == 0 || heads[i-1].light[3] == 0
                || heads[i-1].path[2] != h.path[0] || heads[i-1].path[3] != h.path[1]))
                qInfo("Matrix bolt fork root: %.3f,%.3f, stamp %.0f", h.path[0], h.path[1], h.light[2]);
        }
#endif
        d->dirty = true;
    }
    markDirty(DirtyForceUpdate);
}
void RainBolts::prepare()
{
    auto* cb = commandBuffer();
    if (!cb || !d->target || !d->count) return;
    auto* changes = d->rhi->nextResourceUpdateBatch();
    d->records->commitTextureOperations(d->rhi, changes);
    d->atlas->commitTextureOperations(d->rhi, changes);
    const auto clearFailedPass = [&] {
        // Submit the queued uploads (their dirty flags are already cleared)
        // and make the attached display transparent until a draw can succeed.
        cb->beginPass(d->target->target(), Qt::transparent, {1,0}, changes);
        cb->endPass();
    };
    if (!d->bindings) {
        d->bindings.reset(d->rhi->newShaderResourceBindings());
        const auto vert = QRhiShaderResourceBinding::VertexStage, frag = QRhiShaderResourceBinding::FragmentStage;
        d->bindings->setBindings({QRhiShaderResourceBinding::uniformBuffer(0, vert|frag, d->uniforms.get()),
            QRhiShaderResourceBinding::sampledTexture(1, vert, d->records->rhiTexture(), d->sampler.get()),
            QRhiShaderResourceBinding::sampledTexture(2, frag, d->atlas->rhiTexture(), d->sampler.get())});
        if (!d->bindings->create()) { d->bindings.reset(); clearFailedPass(); return; }
        d->pipeline.reset(d->rhi->newGraphicsPipeline());
        d->pipeline->setShaderStages({{QRhiShaderStage::Vertex, RainMaterial::vertexShader(true)},
            {QRhiShaderStage::Fragment, RainMaterial::fragmentShader(true)}});
        QRhiVertexInputLayout layout;
        layout.setBindings({{16}});
        layout.setAttributes({{0,0,QRhiVertexInputAttribute::Float2,0}, {0,1,QRhiVertexInputAttribute::Float2,8}});
        d->pipeline->setVertexInputLayout(layout);
        d->pipeline->setShaderResourceBindings(d->bindings.get());
        d->pipeline->setRenderPassDescriptor(d->target->target()->renderPassDescriptor());
        QRhiGraphicsPipeline::TargetBlend blend;
        blend.enable = true;
        blend.opColor = blend.opAlpha = QRhiGraphicsPipeline::Max;
        blend.srcColor = blend.dstColor = blend.srcAlpha = blend.dstAlpha = QRhiGraphicsPipeline::One;
        d->pipeline->setTargetBlends({blend});
        if (!d->pipeline->create()) {
            d->pipeline.reset(); d->bindings.reset(); clearFailedPass(); return;
        }
    }
    QMatrix4x4 matrix;
    if (d->rhi->isYUpInFramebuffer()) matrix.scale(1,-1);
    matrix.ortho(float(d->rect.left()), float(d->rect.right()), float(d->rect.bottom()), float(d->rect.top()), -1, 1);
    matrix = d->rhi->clipSpaceCorrMatrix()*matrix;
    changes->updateDynamicBuffer(d->uniforms.get(), 0, 64, matrix.constData());
    changes->updateDynamicBuffer(d->uniforms.get(), 64, sizeof(d->data), &d->data);
    if (d->dirty) {
        changes->updateDynamicBuffer(d->vertices.get(), 0, d->vertexBytes.size(), d->vertexBytes.constData());
        d->dirty = false;
    }
    cb->beginPass(d->target->target(), Qt::transparent, {1,0}, changes);
    cb->setGraphicsPipeline(d->pipeline.get());
    cb->setViewport({0,0,float(d->pixels.width()),float(d->pixels.height())});
    cb->setShaderResources(d->bindings.get());
    const QRhiCommandBuffer::VertexInput input(d->vertices.get(),0);
    cb->setVertexInput(0,1,&input);
    cb->draw(quint32(d->count*6));
    cb->endPass();
}

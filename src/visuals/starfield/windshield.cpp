// SPDX-License-Identifier: GPL-3.0-or-later
#include "windshield.h"
#include "livingvisualitem.h"
#include <QFile>
#ifdef PVS_ENABLE_VISUAL_CAPTURE
#include <QElapsedTimer>
#endif
#include <cstring>
#include <vector>

namespace
{
struct Block {
    float matrix[16];
    vd_star_optics optics;
    uint32_t counts[4];
    float opacity[4];
    uint32_t groups[512];
    vd_sky_sprite sprites[VD_STAR_SPRITES];
};
static_assert(sizeof(Block) == 3216);
const std::array<QShader, 2>& shaderPack()
{
    static const auto pack = [] {
        std::array<QShader, 2> shaders;
        const char* names[] = { "windshield.vert", "windshield.frag" };
        for (int i = 0; i < 2; ++i) {
            QFile file(QStringLiteral(":/depth/starfield/") + QString::fromLatin1(names[i]) + QStringLiteral(".qsb"));
            if (file.open(QIODevice::ReadOnly))
                shaders[i] = QShader::fromSerialized(file.readAll());
        }
        return shaders;
    }();
    return pack;
}
}
Windshield::Windshield(LivingVisualItem* item, QRhi* rhi)
    : m_item(item)
    , m_rhi(rhi)
{
}
void Windshield::sync(const VisualSampleLease& sample, const vd_sample_info& info)
{
    m_sample = sample;
    m_rect = QRectF(0, 0, info.view.viewport.width, info.view.viewport.height);
    markDirty(DirtyMaterial);
}
bool Windshield::createResources()
{
    const auto& pack = shaderPack();
    for (const auto& shader : pack)
        if (!shader.isValid())
            return false;
    m_stars.reset(m_rhi->newTexture(QRhiTexture::RGBA32F, QSize(int(m_packet.row_capacity * 2), VD_STAR_ROWS)));
    m_noise.reset(m_rhi->newTexture(QRhiTexture::RGBA8, QSize(256, 256)));
    m_block.reset(m_rhi->newBuffer(QRhiBuffer::Dynamic, QRhiBuffer::UniformBuffer, sizeof(Block)));
    m_nearest.reset(m_rhi->newSampler(QRhiSampler::Nearest, QRhiSampler::Nearest, QRhiSampler::None,
        QRhiSampler::ClampToEdge, QRhiSampler::ClampToEdge));
    m_linear.reset(m_rhi->newSampler(
        QRhiSampler::Linear, QRhiSampler::Linear, QRhiSampler::None, QRhiSampler::Repeat, QRhiSampler::Repeat));
    if (!m_stars->create() || !m_noise->create() || !m_block->create() || !m_nearest->create() || !m_linear->create())
        return false;
    using Binding = QRhiShaderResourceBinding;
    m_bindings.reset(m_rhi->newShaderResourceBindings());
    m_bindings->setBindings({ Binding::uniformBuffer(0, Binding::VertexStage | Binding::FragmentStage, m_block.get()),
        Binding::sampledTexture(1, Binding::VertexStage, m_stars.get(), m_nearest.get()),
        Binding::sampledTexture(2, Binding::FragmentStage, m_noise.get(), m_linear.get()) });
    if (!m_bindings->create())
        return false;
    auto* target = renderTarget();
    m_pass = target->renderPassDescriptor();
    m_pipeline.reset(m_rhi->newGraphicsPipeline());
    m_pipeline->setFlags(QRhiGraphicsPipeline::UsesScissor);
    m_pipeline->setTopology(QRhiGraphicsPipeline::TriangleStrip);
    m_pipeline->setShaderStages({ { QRhiShaderStage::Vertex, pack[0] }, { QRhiShaderStage::Fragment, pack[1] } });
    m_pipeline->setShaderResourceBindings(m_bindings.get());
    m_pipeline->setRenderPassDescriptor(m_pass);
    m_pipeline->setSampleCount(target->sampleCount());
    QRhiGraphicsPipeline::TargetBlend blend;
    blend.enable = true;
    blend.srcColor = blend.srcAlpha = QRhiGraphicsPipeline::One;
    blend.dstColor = blend.dstAlpha = QRhiGraphicsPipeline::OneMinusSrcAlpha;
    m_pipeline->setTargetBlends({ blend });
#ifdef PVS_ENABLE_VISUAL_CAPTURE
    QElapsedTimer timer;
    timer.start();
#endif
    if (!m_pipeline->create())
        return false;
#ifdef PVS_ENABLE_VISUAL_CAPTURE
    qDebug("Starfield windshield pipeline creation %.3f ms", timer.nsecsElapsed() * 1e-6);
#endif
    qInfo("Visuals Starfield: GPU-projected streaming star cells");
    return true;
}
void Windshield::releaseResources()
{
    m_pipeline.reset();
    m_bindings.reset();
    m_nearest.reset();
    m_linear.reset();
    m_stars.reset();
    m_noise.reset();
    m_block.reset();
    m_pass = nullptr;
    m_resident.fill(0);
}
void Windshield::prepare()
{
    if (m_failed || !m_sample)
        return;
    if (m_pass && m_pass != renderTarget()->renderPassDescriptor())
        releaseResources();
    if (!visuals_starfield_sample(m_sample.get(), m_resident.data(), &m_packet))
        return;
    if (m_stars && m_stars->pixelSize().width() < int(m_packet.row_capacity * 2)) {
        releaseResources();
        if (!visuals_starfield_sample(m_sample.get(), m_resident.data(), &m_packet))
            return;
    }
    const bool first = !m_pass;
    if (first && !createResources()) {
        m_failed = true;
        releaseResources();
        Q_EMIT m_item->gpuUnavailable();
        return;
    }
    auto* update = m_rhi->nextResourceUpdateBatch();
    if (first) {
        QRhiTextureSubresourceUploadDescription data(visuals_starfield_noise(), 256 * 256 * 4);
        data.setSourceSize(QSize(256, 256));
        update->uploadTexture(m_noise.get(), QRhiTextureUploadDescription({ QRhiTextureUploadEntry(0, 0, data) }));
        // At activation all rows are cold. Submit one contiguous image per
        // window instead of a GL subimage call for every visible cell.
        const qsizetype stride = qsizetype(m_packet.row_capacity) * sizeof(vd_star);
        QByteArray image(stride * VD_STAR_ROWS, '\0');
        for (uint32_t i = 0; i < m_packet.updates; ++i) {
            const auto& row = m_packet.upload[i];
            std::memcpy(image.data() + row.row * stride, row.stars, row.count * sizeof(vd_star));
            m_resident[row.row] = row.stamp;
        }
        QRhiTextureSubresourceUploadDescription atlas(image);
        atlas.setSourceSize(m_stars->pixelSize());
        update->uploadTexture(m_stars.get(), QRhiTextureUploadDescription({ QRhiTextureUploadEntry(0, 0, atlas) }));
    } else if (m_packet.updates) {
        std::vector<QRhiTextureUploadEntry> rows;
        rows.reserve(m_packet.updates);
        for (uint32_t i = 0; i < m_packet.updates; ++i) {
            const auto& row = m_packet.upload[i];
            // Groups encode their last four-star unit. Zero the <=3 spare lanes.
            const auto padded = (row.count + 3u) & ~3u;
            QByteArray bytes(reinterpret_cast<const char*>(row.stars), row.count * sizeof(vd_star));
            bytes.resize(padded * sizeof(vd_star), '\0');
            QRhiTextureSubresourceUploadDescription data(bytes);
            data.setSourceSize(QSize(int(2 * padded), 1));
            data.setDestinationTopLeft(QPoint(0, int(row.row)));
            rows.emplace_back(0, 0, data);
            m_resident[row.row] = row.stamp;
        }
        QRhiTextureUploadDescription upload;
        upload.setEntries(rows.begin(), rows.end());
        update->uploadTexture(m_stars.get(), upload);
    }
    Block block {};
    const auto m = *projectionMatrix() * *matrix();
    std::memcpy(block.matrix, m.constData(), sizeof(block.matrix));
    block.optics = m_packet.optics;
    block.counts[0] = m_packet.group_count;
    block.counts[1] = m_packet.sprite_count;
    block.counts[2] = m_packet.debris;
    block.counts[3] = m_packet.near_group_count;
    block.opacity[0] = float(inheritedOpacity());
    std::memcpy(block.groups, m_packet.groups, sizeof(block.groups));
    std::memcpy(block.sprites, m_packet.sprites, m_packet.sprite_count * sizeof(vd_sky_sprite));
    update->updateDynamicBuffer(m_block.get(), 0, sizeof(Block), &block);
    commandBuffer()->resourceUpdate(update);
}
void Windshield::render(const RenderState* state)
{
    if (m_failed || !m_pass)
        return;
    auto* cb = commandBuffer();
    const auto size = renderTarget()->pixelSize();
    const auto rect = state->scissorEnabled() ? state->scissorRect() : QRect(QPoint(), size);
    // Sky instances precede the streaming stars in the same blended draw.
    const uint32_t count = m_packet.sprite_count + (m_packet.debris ? 43 : 0) + m_packet.group_count * 128;
    cb->setGraphicsPipeline(m_pipeline.get());
    cb->setViewport({ 0, 0, float(size.width()), float(size.height()) });
    cb->setScissor({ rect.x(), rect.y(), rect.width(), rect.height() });
    cb->setShaderResources(m_bindings.get());
    cb->setVertexInput(0, 0, nullptr);
    cb->draw(4, count);
}

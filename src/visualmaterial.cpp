// SPDX-License-Identifier: GPL-3.0-or-later
#include "visualmaterial.h"
#include <QSGMaterialShader>
#include <QQuickWindow>
#include <QFile>
#include <rhi/qrhi.h>
#include <cstring>
#include <array>

static void initializeVisualShaders() { Q_INIT_RESOURCE(visuals_shaders); }
namespace {
struct ShaderPack { QShader vertex, fragment; };
const ShaderPack &pack(VisualMaterial::Technique technique)
{
    static const auto packs = [] {
        initializeVisualShaders();
        auto load = [](const char *name) {
            QFile file(QStringLiteral(":/visuals/shaders/") + QString::fromLatin1(name) + QStringLiteral(".qsb"));
            return file.open(QIODevice::ReadOnly) ? QShader::fromSerialized(file.readAll()) : QShader{};
        };
        return std::array<ShaderPack,2>{{{load("aurora.vert"),load("aurora.frag")},
                                      {load("visual-data.vert"),load("visual-data.frag")}}};
    }();
    return packs[technique];
}
class VisualShader final : public QSGMaterialShader
{
public:
    explicit VisualShader(VisualMaterial::Technique technique) {
        setShader(VertexStage,pack(technique).vertex);
        setShader(FragmentStage,pack(technique).fragment);
    }
    bool updateUniformData(RenderState &state, QSGMaterial *material, QSGMaterial *) override {
        auto *m=static_cast<VisualMaterial *>(material);
        char *data=state.uniformData()->data();
        if (state.isMatrixDirty()) std::memcpy(data,state.combinedMatrix().constData(),64);
        const float info[]={state.opacity(),float(m->snapshot.band_count),m->snapshot.viewport[0],m->snapshot.viewport[1]};
        std::memcpy(data+64,info,sizeof(info));
        std::memcpy(data+80,m->snapshot.bands,sizeof(m->snapshot.bands));
        std::memcpy(data+512,m->snapshot.shade,sizeof(m->snapshot.shade));
        return true;
    }
    void updateSampledImage(RenderState &state, int binding, QSGTexture **texture,
                            QSGMaterial *material, QSGMaterial *) override {
        if (binding != 1) return;
        *texture=static_cast<VisualMaterial *>(material)->dataTexture;
        (*texture)->commitTextureOperations(state.rhi(),state.resourceUpdateBatch());
    }
};
}
VisualDataTexture::VisualDataTexture() { setFiltering(Nearest); setHorizontalWrapMode(ClampToEdge); setVerticalWrapMode(ClampToEdge); }
VisualDataTexture::~VisualDataTexture() = default;
qint64 VisualDataTexture::comparisonKey() const { return qint64(quintptr(this)); }
QRhiTexture *VisualDataTexture::rhiTexture() const { return m_texture.get(); }
void VisualDataTexture::setData(const QByteArray &data, QSize size) {
    if (size.width() <= 0 || size.height() <= 0 || qint64(size.width())*size.height()*16 != data.size()) return;
    if (size != m_size) m_texture.reset();
    m_size=size; m_data=data; m_dirty=true;
}
void VisualDataTexture::commitTextureOperations(QRhi *rhi, QRhiResourceUpdateBatch *updates) {
    if (!m_texture) {
        m_texture.reset(rhi->newTexture(QRhiTexture::RGBA32F,m_size));
        if (!m_texture->create()) { m_texture.reset(); qWarning("Visuals: RGBA32F data texture creation failed"); return; }
    }
    if (!m_dirty) return;
    updates->uploadTexture(m_texture.get(),QRhiTextureUploadDescription({
        QRhiTextureUploadEntry(0,0,QRhiTextureSubresourceUploadDescription(m_data))}));
    m_dirty=false;
}
VisualMaterial::VisualMaterial(Technique t) : technique(t) {
    setFlag(Blending); setFlag(RequiresFullMatrix); setFlag(NoBatching);
    if (t==VertexTextureProof) dataTexture=new VisualDataTexture;
}
VisualMaterial::~VisualMaterial() { delete dataTexture; }
QSGMaterialType *VisualMaterial::type() const { static QSGMaterialType types[2]; return &types[technique]; }
QSGMaterialShader *VisualMaterial::createShader(QSGRendererInterface::RenderMode) const { return new VisualShader(technique); }
int VisualMaterial::compare(const QSGMaterial *other) const {
    if (other == this) return 0;
    return std::less<const QSGMaterial *>{}(this,other) ? -1 : 1;
}
bool VisualMaterial::shadersAvailable(Technique t) { return pack(t).vertex.isValid() && pack(t).fragment.isValid(); }
bool VisualMaterial::supportsWindow(QQuickWindow *window, Technique t) {
    if (!window || !shadersAvailable(t)) return false;
    auto *ri=window->rendererInterface();
    auto *rhi=static_cast<QRhi *>(ri->getResource(window,QSGRendererInterface::RhiResource));
    if (!rhi || rhi->backend()==QRhi::Null) return false;
    auto *target=static_cast<QRhiRenderTarget *>(ri->getResource(window,QSGRendererInterface::RhiRedirectRenderTarget));
    if (!target) {
        auto *sc=static_cast<QRhiSwapChain *>(ri->getResource(window,QSGRendererInterface::RhiSwapchainResource));
        if (sc) target=sc->currentFrameRenderTarget();
    }
    if (!target || !target->renderPassDescriptor()) return false;
    std::unique_ptr<QRhiBuffer> uniforms(rhi->newBuffer(QRhiBuffer::Dynamic,QRhiBuffer::UniformBuffer,UniformSize));
    if (!uniforms->create()) return false;
    std::unique_ptr<QRhiShaderResourceBindings> bindings(rhi->newShaderResourceBindings());
    std::unique_ptr<QRhiTexture> texture;
    std::unique_ptr<QRhiSampler> sampler;
    const auto stages=QRhiShaderResourceBinding::VertexStage | QRhiShaderResourceBinding::FragmentStage;
    if (t==VertexTextureProof) {
        if (!rhi->isTextureFormatSupported(QRhiTexture::RGBA32F)) return false;
        texture.reset(rhi->newTexture(QRhiTexture::RGBA32F,{2,1}));
        sampler.reset(rhi->newSampler(QRhiSampler::Nearest,QRhiSampler::Nearest,QRhiSampler::None,
            QRhiSampler::ClampToEdge,QRhiSampler::ClampToEdge));
        if (!texture->create() || !sampler->create()) return false;
        bindings->setBindings({QRhiShaderResourceBinding::uniformBuffer(0,stages,uniforms.get()),
            QRhiShaderResourceBinding::sampledTexture(1,QRhiShaderResourceBinding::VertexStage,texture.get(),sampler.get())});
    } else bindings->setBindings({QRhiShaderResourceBinding::uniformBuffer(0,stages,uniforms.get())});
    if (!bindings->create()) return false;
    QRhiVertexInputLayout layout;
    layout.setBindings({{16}});
    layout.setAttributes({{0,0,QRhiVertexInputAttribute::Float2,0},{0,1,QRhiVertexInputAttribute::Float2,8}});
    std::unique_ptr<QRhiGraphicsPipeline> pipeline(rhi->newGraphicsPipeline());
    pipeline->setShaderStages({{QRhiShaderStage::Vertex,pack(t).vertex},{QRhiShaderStage::Fragment,pack(t).fragment}});
    pipeline->setVertexInputLayout(layout);
    pipeline->setShaderResourceBindings(bindings.get());
    pipeline->setRenderPassDescriptor(target->renderPassDescriptor());
    pipeline->setSampleCount(target->sampleCount());
    QRhiGraphicsPipeline::TargetBlend blend;
    blend.enable=true; blend.srcColor=blend.srcAlpha=QRhiGraphicsPipeline::One;
    blend.dstColor=blend.dstAlpha=QRhiGraphicsPipeline::OneMinusSrcAlpha;
    pipeline->setTargetBlends({blend});
    return pipeline->create();
}

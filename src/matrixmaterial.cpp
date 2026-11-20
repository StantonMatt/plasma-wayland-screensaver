// SPDX-License-Identifier: GPL-3.0-or-later
#include "matrixmaterial.h"
#include <QQuickWindow>
#include <QSGMaterialShader>
#include <QFile>
#include <rhi/qrhi.h>
#include <cstring>
static void initializeMatrixShaders() {Q_INIT_RESOURCE(matrix_shaders);}
namespace {
struct Pack {QShader vertex,fragment;};
const Pack &pack(bool plain) {
    static const auto packs=[] {
        initializeMatrixShaders();
        auto load=[](const char *name) {QFile f(QStringLiteral(":/matrix/shaders/")+QString::fromLatin1(name)+QStringLiteral(".qsb"));return f.open(QIODevice::ReadOnly)?QShader::fromSerialized(f.readAll()):QShader{};};
        return std::array<Pack,2>{{{load("matrix.vert"),load("matrix.frag")},{load("matrix-plain.vert"),load("matrix.frag")}}};
    }();
    return packs[plain];
}
class Shader final : public QSGMaterialShader {
public:
    explicit Shader(bool plain) {setShader(VertexStage,pack(plain).vertex);setShader(FragmentStage,pack(plain).fragment);}
    bool updateUniformData(RenderState &state,QSGMaterial *material,QSGMaterial *) override {
        auto *m=static_cast<MatrixMaterial *>(material);auto *p=state.uniformData()->data();
        if(state.isMatrixDirty()) std::memcpy(p,state.combinedMatrix().constData(),64);
        const float info[]={state.opacity(),float(m->columns),0,0};std::memcpy(p+64,info,16);return true;
    }
    void updateSampledImage(RenderState &state,int binding,QSGTexture **texture,QSGMaterial *material,QSGMaterial *) override {
        auto *m=static_cast<MatrixMaterial *>(material);
        if(binding==1 && !m->plain) *texture=&m->offsets;
        else if(binding==2) *texture=m->atlas;
        else return;
        if(*texture) (*texture)->commitTextureOperations(state.rhi(),state.resourceUpdateBatch());
    }
};
}
MatrixAtlasTexture::MatrixAtlasTexture(const QImage &alpha):m_pending(alpha),m_size(alpha.size()) {
    Q_ASSERT(alpha.format()==QImage::Format_Alpha8);
    setFiltering(Linear);setMipmapFiltering(Linear);setHorizontalWrapMode(ClampToEdge);setVerticalWrapMode(ClampToEdge);
}
MatrixAtlasTexture::~MatrixAtlasTexture()=default;
qint64 MatrixAtlasTexture::comparisonKey()const{return qint64(quintptr(this));}
QRhiTexture *MatrixAtlasTexture::rhiTexture()const{return m_texture.get();}
void MatrixAtlasTexture::commitTextureOperations(QRhi *rhi,QRhiResourceUpdateBatch *updates){
    if(!m_texture){
        m_texture.reset(rhi->newTexture(QRhiTexture::R8,m_size,1,QRhiTexture::MipMapped|QRhiTexture::UsedWithGenerateMips));
        if(!m_texture->create()){m_texture.reset();qWarning("Visuals Matrix: R8 coverage texture creation failed");return;}
    }
    if(m_pending.isNull())return;
    // Retain owned upload bytes until QRhi consumes the batch. Unlike Qt's
    // generic image texture this needs no RGBA conversion/retained RGBA copy.
    QRhiTextureSubresourceUploadDescription upload(QByteArray(reinterpret_cast<const char *>(m_pending.constBits()),m_pending.sizeInBytes()));
    upload.setSourceSize(m_size);upload.setDataStride(m_pending.bytesPerLine());
    updates->uploadTexture(m_texture.get(),QRhiTextureUploadDescription({QRhiTextureUploadEntry(0,0,upload)}));
    updates->generateMips(m_texture.get());m_pending=QImage();
}
MatrixMaterial::MatrixMaterial(bool p):plain(p) {setFlag(Blending);setFlag(RequiresFullMatrix);setFlag(NoBatching);}
MatrixMaterial::~MatrixMaterial(){delete atlas;}
QSGMaterialType *MatrixMaterial::type()const {static QSGMaterialType types[2];return &types[plain];}
QSGMaterialShader *MatrixMaterial::createShader(QSGRendererInterface::RenderMode)const{return new Shader(plain);}
int MatrixMaterial::compare(const QSGMaterial *other)const {return other==this?0:std::less<const QSGMaterial *>{}(this,other)?-1:1;}
bool MatrixMaterial::shadersAvailable(bool p){return pack(p).vertex.isValid()&&pack(p).fragment.isValid();}
bool MatrixMaterial::supportsWindow(QQuickWindow *window,int columns,bool plain,QSize atlasSize) {
    if(!window||!shadersAvailable(plain))return false;
    auto *ri=window->rendererInterface();auto *rhi=static_cast<QRhi *>(ri->getResource(window,QSGRendererInterface::RhiResource));
    if(!rhi||rhi->backend()==QRhi::Null)return false;
    auto *target=static_cast<QRhiRenderTarget *>(ri->getResource(window,QSGRendererInterface::RhiRedirectRenderTarget));
    if(!target){auto *sc=static_cast<QRhiSwapChain *>(ri->getResource(window,QSGRendererInterface::RhiSwapchainResource));if(sc)target=sc->currentFrameRenderTarget();}
    if(!target||!target->renderPassDescriptor())return false;
    const int limit=rhi->resourceLimit(QRhi::TextureSizeMax);
    if(columns<=0||columns>limit||atlasSize.isEmpty()||atlasSize.width()>limit||atlasSize.height()>limit)return false;
    std::unique_ptr<QRhiBuffer> uniforms(rhi->newBuffer(QRhiBuffer::Dynamic,QRhiBuffer::UniformBuffer,80));
    if(!rhi->isTextureFormatSupported(QRhiTexture::R8))return false;
    std::unique_ptr<QRhiTexture> atlas(rhi->newTexture(QRhiTexture::R8,atlasSize,1,QRhiTexture::MipMapped|QRhiTexture::UsedWithGenerateMips)),offsets;
    std::unique_ptr<QRhiSampler> linear(rhi->newSampler(QRhiSampler::Linear,QRhiSampler::Linear,QRhiSampler::Linear,QRhiSampler::ClampToEdge,QRhiSampler::ClampToEdge));
    std::unique_ptr<QRhiSampler> nearest(rhi->newSampler(QRhiSampler::Nearest,QRhiSampler::Nearest,QRhiSampler::None,QRhiSampler::ClampToEdge,QRhiSampler::ClampToEdge));
    if(!uniforms->create()||!atlas->create()||!linear->create()||!nearest->create())return false;
    std::unique_ptr<QRhiShaderResourceBindings> bindings(rhi->newShaderResourceBindings());
    const auto stages=QRhiShaderResourceBinding::VertexStage|QRhiShaderResourceBinding::FragmentStage;
    if(!plain) {
        if(!rhi->isTextureFormatSupported(QRhiTexture::RGBA32F))return false;
        offsets.reset(rhi->newTexture(QRhiTexture::RGBA32F,{columns,1}));if(!offsets->create())return false;
        bindings->setBindings({QRhiShaderResourceBinding::uniformBuffer(0,stages,uniforms.get()),
            QRhiShaderResourceBinding::sampledTexture(1,QRhiShaderResourceBinding::VertexStage,offsets.get(),nearest.get()),
            QRhiShaderResourceBinding::sampledTexture(2,QRhiShaderResourceBinding::FragmentStage,atlas.get(),linear.get())});
    }else bindings->setBindings({QRhiShaderResourceBinding::uniformBuffer(0,stages,uniforms.get()),QRhiShaderResourceBinding::sampledTexture(2,QRhiShaderResourceBinding::FragmentStage,atlas.get(),linear.get())});
    if(!bindings->create())return false;
    QRhiVertexInputLayout layout;layout.setBindings({{48}});
    layout.setAttributes({{0,0,QRhiVertexInputAttribute::Float2,0},{0,1,QRhiVertexInputAttribute::Float4,8},{0,2,QRhiVertexInputAttribute::Float4,24},{0,3,QRhiVertexInputAttribute::Float,40}});
    std::unique_ptr<QRhiGraphicsPipeline> pipeline(rhi->newGraphicsPipeline());
    pipeline->setShaderStages({{QRhiShaderStage::Vertex,pack(plain).vertex},{QRhiShaderStage::Fragment,pack(plain).fragment}});
    pipeline->setVertexInputLayout(layout);pipeline->setShaderResourceBindings(bindings.get());pipeline->setRenderPassDescriptor(target->renderPassDescriptor());pipeline->setSampleCount(target->sampleCount());
    QRhiGraphicsPipeline::TargetBlend blend;blend.enable=true;blend.srcColor=blend.srcAlpha=QRhiGraphicsPipeline::One;blend.dstColor=blend.dstAlpha=QRhiGraphicsPipeline::OneMinusSrcAlpha;pipeline->setTargetBlends({blend});
    return pipeline->create();
}

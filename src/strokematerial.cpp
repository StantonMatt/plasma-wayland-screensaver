// SPDX-License-Identifier: GPL-3.0-or-later
#include "strokematerial.h"
#include "visuals_strokes.h"
#include <QSGMaterialShader>
#include <QQuickWindow>
#include <QFile>
#include <rhi/qrhi.h>
#include <memory>
#include <cstring>
static void initializeStrokeShaders() {Q_INIT_RESOURCE(strokes_shaders);}
namespace {
struct Pack {QShader vertex,fragment;};
const Pack &pack() {
    static const Pack result=[] {
        initializeStrokeShaders();
        auto load=[](const char *name) {
            QFile file(QStringLiteral(":/visuals/shaders/")+QString::fromLatin1(name)+QStringLiteral(".qsb"));
            return file.open(QIODevice::ReadOnly)?QShader::fromSerialized(file.readAll()):QShader{};
        };
        return Pack{load("strokes.vert"),load("strokes.frag")};
    }();return result;
}
class Shader final : public QSGMaterialShader {
public:
    Shader() {
        setShader(VertexStage,pack().vertex);setShader(FragmentStage,pack().fragment);
        setFlag(UpdatesGraphicsPipelineState);
    }
    bool updateUniformData(RenderState &state,QSGMaterial *,QSGMaterial *) override {
        auto *data=state.uniformData()->data();
        if (state.isMatrixDirty()) std::memcpy(data,state.combinedMatrix().constData(),64);
        const float info[]={state.opacity(),state.devicePixelRatio(),0,0};
        std::memcpy(data+64,info,sizeof(info));return true;
    }
    bool updateGraphicsPipelineState(RenderState &,GraphicsPipelineState *ps,QSGMaterial *,QSGMaterial *) override {
        ps->blendEnable=true;ps->separateBlendFactors=true;
        ps->srcColor=ps->dstColor=ps->srcAlpha=ps->dstAlpha=GraphicsPipelineState::One;
        ps->opColor=ps->opAlpha=GraphicsPipelineState::BlendOp::Add;
        return true;
    }
};
}
StrokeMaterial::StrokeMaterial() {setFlag(Blending);setFlag(RequiresFullMatrix);setFlag(NoBatching);}
QSGMaterialType *StrokeMaterial::type() const {static QSGMaterialType type;return &type;}
QSGMaterialShader *StrokeMaterial::createShader(QSGRendererInterface::RenderMode) const {return new Shader;}
int StrokeMaterial::compare(const QSGMaterial *other) const {
    if (other==this) return 0;
    return std::less<const QSGMaterial *>{}(this,other)?-1:1;
}
bool StrokeMaterial::shadersAvailable() {return pack().vertex.isValid() && pack().fragment.isValid();}
QShader StrokeMaterial::vertexShader() {return pack().vertex;}
QShader StrokeMaterial::fragmentShader() {return pack().fragment;}
bool StrokeMaterial::supportsWindow(QQuickWindow *window) {
    if (!window || !shadersAvailable()) return false;
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
    bindings->setBindings({QRhiShaderResourceBinding::uniformBuffer(0,
        QRhiShaderResourceBinding::VertexStage|QRhiShaderResourceBinding::FragmentStage,uniforms.get())});
    if (!bindings->create()) return false;
    QRhiVertexInputLayout layout;layout.setBindings({{sizeof(visuals_strokes_vertex)}});
    layout.setAttributes({{0,0,QRhiVertexInputAttribute::Float2,0},{0,1,QRhiVertexInputAttribute::Float4,8},
                          {0,2,QRhiVertexInputAttribute::Float2,24},{0,3,QRhiVertexInputAttribute::Float4,32}});
    std::unique_ptr<QRhiGraphicsPipeline> pipeline(rhi->newGraphicsPipeline());
    pipeline->setShaderStages({{QRhiShaderStage::Vertex,pack().vertex},{QRhiShaderStage::Fragment,pack().fragment}});
    pipeline->setVertexInputLayout(layout);pipeline->setShaderResourceBindings(bindings.get());
    pipeline->setRenderPassDescriptor(target->renderPassDescriptor());pipeline->setSampleCount(target->sampleCount());
    QRhiGraphicsPipeline::TargetBlend blend;blend.enable=true;
    blend.srcColor=blend.dstColor=blend.srcAlpha=blend.dstAlpha=QRhiGraphicsPipeline::One;
    pipeline->setTargetBlends({blend});return pipeline->create();
}

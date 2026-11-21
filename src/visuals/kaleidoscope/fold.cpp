// SPDX-License-Identifier: GPL-3.0-or-later
#include "fold.h"
#include <QFile>
#include <QQuickWindow>
#include <QSGMaterial>
#include <QSGMaterialShader>
#include <cstring>
#include <memory>

QShader kaleidoscopeShader(const char* name)
{
    QFile file(QStringLiteral(":/depth/kaleidoscope/") + QString::fromLatin1(name) + QStringLiteral(".qsb"));
    return file.open(QIODevice::ReadOnly) ? QShader::fromSerialized(file.readAll()) : QShader();
}
namespace
{
const QShader& vertex()
{
    static const auto s = kaleidoscopeShader("fold.vert");
    return s;
}
const QShader& fragment()
{
    static const auto s = kaleidoscopeShader("fold.frag");
    return s;
}

class Material final : public QSGMaterial
{
public:
    Material(QSGTexture* sharp, QSGTexture* soft)
        : chamber(sharp)
        , bloom(soft)
    {
        setFlag(RequiresFullMatrix);
        setFlag(NoBatching);
        // The fragment emits premultiplied light with zero coverage. Qt's
        // One, OneMinusSrcAlpha blend adds it over the real QML background.
        setFlag(Blending);
    }
    QSGMaterialType* type() const override
    {
        static QSGMaterialType t;
        return &t;
    }
    QSGMaterialShader* createShader(QSGRendererInterface::RenderMode) const override;
    QSGTexture* chamber;
    QSGTexture* bloom;
    kaleidoscope_lens lens {};
};
class Shader final : public QSGMaterialShader
{
public:
    Shader()
    {
        setShader(VertexStage, vertex());
        setShader(FragmentStage, fragment());
    }
    bool updateUniformData(RenderState& state, QSGMaterial* material, QSGMaterial*) override
    {
        auto* m = static_cast<const Material*>(material);
        char* data = state.uniformData()->data();
        if (state.isMatrixDirty())
            std::memcpy(data, state.combinedMatrix().constData(), 64);
        std::memcpy(data + 64, &m->lens, sizeof(m->lens));
        const float opacity = state.opacity();
        std::memcpy(data + 64 + sizeof(m->lens), &opacity, sizeof(opacity));
        return true;
    }
    void updateSampledImage(
        RenderState& s, int binding, QSGTexture** tex, QSGMaterial* material, QSGMaterial*) override
    {
        auto* m = static_cast<const Material*>(material);
        if (binding == 1 || binding == 2) {
            *tex = binding == 1 ? m->chamber : m->bloom;
            (*tex)->commitTextureOperations(s.rhi(), s.resourceUpdateBatch());
        }
    }
};
QSGMaterialShader* Material::createShader(QSGRendererInterface::RenderMode) const
{
    return new Shader;
}
}
FoldNode::FoldNode(QSGTexture* sharp, QSGTexture* soft, QSizeF size)
{
    auto* g = new QSGGeometry(QSGGeometry::defaultAttributes_Point2D(), 4);
    g->setDrawingMode(QSGGeometry::DrawTriangleStrip);
    QSGGeometry::updateRectGeometry(g, QRectF(QPointF(), size));
    setGeometry(g);
    setFlag(OwnsGeometry);
    setMaterial(new Material(sharp, soft));
    setFlag(OwnsMaterial);
}
void FoldNode::sync(const kaleidoscope_lens& lens)
{
    static_cast<Material*>(material())->lens = lens;
    markDirty(DirtyMaterial);
}
bool FoldNode::supports(QQuickWindow* window, QRhi* rhi, QRhiShaderResourceBindings* bindings)
{
    if (!vertex().isValid() || !fragment().isValid())
        return false;
    auto* ri = window->rendererInterface();
    auto* target
        = static_cast<QRhiRenderTarget*>(ri->getResource(window, QSGRendererInterface::RhiRedirectRenderTarget));
    if (!target) {
        auto* swap
            = static_cast<QRhiSwapChain*>(ri->getResource(window, QSGRendererInterface::RhiSwapchainResource));
        if (swap)
            target = swap->currentFrameRenderTarget();
    }
    if (!target || !target->renderPassDescriptor())
        return false;
    QRhiVertexInputLayout layout;
    layout.setBindings({ { 8 } });
    layout.setAttributes({ { 0, 0, QRhiVertexInputAttribute::Float2, 0 } });
    std::unique_ptr<QRhiGraphicsPipeline> pipe(rhi->newGraphicsPipeline());
    pipe->setVertexInputLayout(layout);
    pipe->setTopology(QRhiGraphicsPipeline::TriangleStrip);
    pipe->setSampleCount(target->sampleCount());
    pipe->setShaderStages({ { QRhiShaderStage::Vertex, vertex() }, { QRhiShaderStage::Fragment, fragment() } });
    pipe->setShaderResourceBindings(bindings);
    pipe->setRenderPassDescriptor(target->renderPassDescriptor());
    return pipe->create();
}

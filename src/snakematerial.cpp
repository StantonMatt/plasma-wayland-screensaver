// SPDX-License-Identifier: GPL-3.0-or-later
#include "snakematerial.h"
#include "snakes_core.h"
#include <QSGMaterialShader>
#include <QSGTexture>
#include "snakes_atlas.h"
#include <QFile>
#include <QOpenGLContext>
#include <QQuickWindow>
#include <rhi/qrhi.h>
#include <memory>
#include <algorithm>
#include <cstring>
#include <cstddef>

static void initializeSnakeShaders() { Q_INIT_RESOURCE(snakes_shaders); }

namespace {
struct ShaderPack {
    QShader vertex;
    QShader fragment;
};
const ShaderPack &shaderPack()
{
    static const ShaderPack pack = [] {
        initializeSnakeShaders();
        auto load = [](const char *path) {
            QFile file(QString::fromLatin1(path));
            return file.open(QIODevice::ReadOnly) ? QShader::fromSerialized(file.readAll()) : QShader{};
        };
        return ShaderPack{load(":/snakes/shaders/snake.vert.qsb"), load(":/snakes/shaders/snake.frag.qsb")};
    }();
    return pack;
}

bool hasPair(const QShader &vertex, const QShader &fragment,
             QShader::Source source, QShaderVersion version)
{
    // Quick can request either variant; the fragment stage is never rewritten.
    const auto vertexKeys = vertex.availableShaders();
    const auto fragmentKeys = fragment.availableShaders();
    for (auto variant : {QShader::StandardShader, QShader::BatchableVertexShader}) {
        const QShaderKey key(source, version, variant);
        if (!vertexKeys.contains(key) || vertex.shader(key).shader().isEmpty()) return false;
    }
    const QShaderKey key(source, version);
    return fragmentKeys.contains(key) && !fragment.shader(key).shader().isEmpty();
}

// One immutable R8 texture per scene-graph material, uploaded once. No runtime
// image conversion or atlas generation; QSGTexture owns the RHI lifecycle.
class IconAtlas final : public QSGTexture
{
public:
    IconAtlas() { setFiltering(Linear); }
    qint64 comparisonKey() const override { return qint64(quintptr(this)); }
    QRhiTexture *rhiTexture() const override { return m_texture.get(); }
    QSize textureSize() const override { return {128, 128}; }
    bool hasAlphaChannel() const override { return false; }
    bool hasMipmaps() const override { return false; }
    void commitTextureOperations(QRhi *rhi, QRhiResourceUpdateBatch *updates) override
    {
        if (m_texture) return;
        m_texture.reset(rhi->newTexture(QRhiTexture::R8, {128, 128}));
        if (!m_texture->create()) { m_texture.reset(); return; }
        const QByteArray pixels(reinterpret_cast<const char *>(snakesIconAtlas), sizeof(snakesIconAtlas));
        updates->uploadTexture(m_texture.get(), QRhiTextureUploadDescription({
            QRhiTextureUploadEntry(0, 0, QRhiTextureSubresourceUploadDescription(pixels))}));
    }
private:
    std::unique_ptr<QRhiTexture> m_texture;
};

class SnakeShader final : public QSGMaterialShader
{
public:
    SnakeShader()
    {
        setShader(VertexStage, shaderPack().vertex);
        setShader(FragmentStage, shaderPack().fragment);
    }
    void updateSampledImage(RenderState &state, int binding, QSGTexture **texture,
                            QSGMaterial *newMaterial, QSGMaterial *) override
    {
        if (binding == SnakeMaterial::IconAtlasBinding) {
            *texture = static_cast<SnakeMaterial *>(newMaterial)->iconAtlas();
            // Custom materials must enqueue their texture creation/upload here;
            // returning a QSGTexture alone does not commit its RHI operations.
            (*texture)->commitTextureOperations(state.rhi(), state.resourceUpdateBatch());
        }
    }
    bool updateUniformData(RenderState &state, QSGMaterial *newMaterial, QSGMaterial *oldMaterial) override
    {
        auto *material = static_cast<SnakeMaterial *>(newMaterial);
        auto *old = static_cast<SnakeMaterial *>(oldMaterial);
        auto *data = state.uniformData();
        bool changed = false;
        if (state.isMatrixDirty()) {
            std::memcpy(data->data(), state.combinedMatrix().constData(), 64);
            changed = true;
        }
        if (state.isOpacityDirty()) {
            const float opacity = state.opacity();
            std::memcpy(data->data() + offsetof(SnakeMaterial::UniformData, opacity), &opacity, 4);
            changed = true;
        }
        // Qt may reuse the same material pointer between updates. Compare the
        // retained uniform bytes so in-place time changes still reach the GPU.
        if (!old || std::memcmp(data->constData() + offsetof(SnakeMaterial::UniformData, time), &material->time, 4) != 0) {
            std::memcpy(data->data() + offsetof(SnakeMaterial::UniformData, time), &material->time, 4);
            changed = true;
        }
        if (!old || std::memcmp(data->constData() + offsetof(SnakeMaterial::UniformData, animationTime), &material->animationTime, 4) != 0
                || std::memcmp(data->constData() + offsetof(SnakeMaterial::UniformData, motionScale), &material->motionScale, 4) != 0) {
            std::memcpy(data->data() + offsetof(SnakeMaterial::UniformData, animationTime), &material->animationTime, 4);
            std::memcpy(data->data() + offsetof(SnakeMaterial::UniformData, motionScale), &material->motionScale, 4);
            changed = true;
        }
        if (!old || std::memcmp(data->constData() + offsetof(SnakeMaterial::UniformData, paletteMode), &material->paletteMode, 4) != 0) {
            std::memcpy(data->data() + offsetof(SnakeMaterial::UniformData, paletteMode), &material->paletteMode, 4);
            changed = true;
        }
        if (!old || std::memcmp(data->constData() + offsetof(SnakeMaterial::UniformData, ambient), &material->ambient, 4) != 0) {
            std::memcpy(data->data() + offsetof(SnakeMaterial::UniformData, ambient), &material->ambient, 4);
            changed = true;
        }
        if (!old) {
            const float light[] = {-0.55f, -0.83f};
            std::memcpy(data->data() + offsetof(SnakeMaterial::UniformData, light), light, 8);
            changed = true;
        }
        return changed;
    }
};
}
SnakeMaterial::SnakeMaterial() : m_iconAtlas(new IconAtlas) { setFlag(Blending); }
SnakeMaterial::~SnakeMaterial() { delete m_iconAtlas; }

QSGMaterialType *SnakeMaterial::type() const { static QSGMaterialType type; return &type; }
QSGMaterialShader *SnakeMaterial::createShader(QSGRendererInterface::RenderMode) const { return new SnakeShader; }
int SnakeMaterial::compare(const QSGMaterial *other) const
{
    const float otherTime = static_cast<const SnakeMaterial *>(other)->time;
    if (time != otherTime) return time < otherTime ? -1 : 1;
    const auto *material = static_cast<const SnakeMaterial *>(other);
    if (animationTime != material->animationTime) return animationTime < material->animationTime ? -1 : 1;
    if (motionScale != material->motionScale) return motionScale < material->motionScale ? -1 : 1;
    if (ambient != material->ambient) return ambient < material->ambient ? -1 : 1;
    return paletteMode < material->paletteMode ? -1 : paletteMode > material->paletteMode ? 1 : 0;
}
const QSGGeometry::AttributeSet &SnakeMaterial::attributes()
{
    static_assert(sizeof(snakes_core_shader_vertex) == 24);
    static const QSGGeometry::Attribute attrs[] = {
        QSGGeometry::Attribute::create(0, 2, QSGGeometry::FloatType, true),
        QSGGeometry::Attribute::create(1, 2, QSGGeometry::FloatType),
        QSGGeometry::Attribute::create(2, 4, QSGGeometry::UnsignedByteType),
        QSGGeometry::Attribute::create(3, 4, QSGGeometry::UnsignedByteType),
    };
    static const QSGGeometry::AttributeSet set = {4, 24, attrs};
    return set;
}
bool SnakeMaterial::shadersAvailable()
{
    const auto &pack = shaderPack();
    return pack.vertex.isValid() && pack.fragment.isValid()
        && pack.vertex.stage() == QShader::VertexStage && pack.fragment.stage() == QShader::FragmentStage;
}

bool SnakeMaterial::shadersSupported(const QShader &vertex, const QShader &fragment,
                                    QSGRendererInterface::GraphicsApi api, const QSurfaceFormat &format)
{
    if (!vertex.isValid() || !fragment.isValid() || vertex.stage() != QShader::VertexStage
        || fragment.stage() != QShader::FragmentStage || !vertex.description().isValid()
        || !fragment.description().isValid()) return false;
    const auto inputs = vertex.description().inputVariables();
    if (std::none_of(inputs.cbegin(), inputs.cend(), [](const auto &input) {
        return input.name == "_qt_order" && input.location == 7;
    })) return false;

    switch (api) {
    case QSGRendererInterface::Vulkan:
        return hasPair(vertex, fragment, QShader::SpirvShader, 100);
    case QSGRendererInterface::Direct3D11:
    case QSGRendererInterface::Direct3D12:
        return hasPair(vertex, fragment, QShader::HlslShader, 50)
            || hasPair(vertex, fragment, QShader::DxbcShader, 50);
    case QSGRendererInterface::Metal:
        return hasPair(vertex, fragment, QShader::MslShader, 12)
            || hasPair(vertex, fragment, QShader::MetalLibShader, 12);
    case QSGRendererInterface::OpenGL: {
        const int major = format.majorVersion(), minor = format.minorVersion();
        if (format.renderableType() == QSurfaceFormat::OpenGLES) {
            for (int version : {320, 310, 300, 100}) {
                if (version != 100 && (major < 3 || (major == 3 && minor < (version - 300) / 10))) continue;
                if (hasPair(vertex, fragment, QShader::GlslShader, {version, QShaderVersion::GlslEs})) return true;
            }
        } else if (format.renderableType() == QSurfaceFormat::OpenGL) {
            // Match Qt 6 RHI's GLSL selection, including GL 3.0/3.1 contexts.
            const int maximum = major >= 4 ? major * 100 + minor * 10
                : major == 3 ? (minor >= 3 ? 330 : 130 + minor * 10) : 120;
            for (int version : {460, 450, 440, 430, 420, 410, 400, 330, 150, 140, 130, 120}) {
                if (version > maximum || (version == 120 && format.profile() == QSurfaceFormat::CoreProfile)) continue;
                if (hasPair(vertex, fragment, QShader::GlslShader, version)) return true;
            }
        }
        return false;
    }
    default:
        return false;
    }
}

bool SnakeMaterial::probePipelines(QRhi *rhi, QRhiRenderTarget *target,
                                  const QShader &vertex, const QShader &fragment)
{
    if (!rhi || !target || !target->renderPassDescriptor() || !vertex.isValid() || !fragment.isValid()) return false;
    // Use the window's actual render pass and sample count, not a new device or
    // a guessed offscreen format. Nothing is submitted and no pixels are read.
    std::unique_ptr<QRhiBuffer> uniforms(rhi->newBuffer(QRhiBuffer::Dynamic, QRhiBuffer::UniformBuffer, UniformBufferSize));
    if (!uniforms->create()) return false;
    std::unique_ptr<QRhiTexture> atlas(rhi->newTexture(QRhiTexture::R8, {128, 128}));
    std::unique_ptr<QRhiSampler> sampler(rhi->newSampler(QRhiSampler::Linear, QRhiSampler::Linear,
        QRhiSampler::None, QRhiSampler::ClampToEdge, QRhiSampler::ClampToEdge));
    if (!atlas->create() || !sampler->create()) return false;
    std::unique_ptr<QRhiShaderResourceBindings> bindings(rhi->newShaderResourceBindings());
    bindings->setBindings({QRhiShaderResourceBinding::uniformBuffer(UniformBinding,
        QRhiShaderResourceBinding::VertexStage | QRhiShaderResourceBinding::FragmentStage, uniforms.get()),
        QRhiShaderResourceBinding::sampledTexture(IconAtlasBinding, QRhiShaderResourceBinding::FragmentStage, atlas.get(), sampler.get())});
    if (!bindings->create()) return false;

    for (auto variant : {QShader::StandardShader, QShader::BatchableVertexShader}) {
        QRhiVertexInputLayout layout;
        layout.setBindings({{24}});
        layout.setAttributes({{0, 0, QRhiVertexInputAttribute::Float2, 0},
                              {0, 1, QRhiVertexInputAttribute::Float2, 8},
                              {0, 2, QRhiVertexInputAttribute::UNormByte4, 16},
                              {0, 3, QRhiVertexInputAttribute::UNormByte4, 20}});
        if (variant == QShader::BatchableVertexShader) {
            layout.setBindings({{24}, {4}});
            layout.setAttributes({{0, 0, QRhiVertexInputAttribute::Float2, 0},
                                  {0, 1, QRhiVertexInputAttribute::Float2, 8},
                                  {0, 2, QRhiVertexInputAttribute::UNormByte4, 16},
                                  {0, 3, QRhiVertexInputAttribute::UNormByte4, 20},
                                  {1, 7, QRhiVertexInputAttribute::Float, 0}});
        }
        std::unique_ptr<QRhiGraphicsPipeline> pipeline(rhi->newGraphicsPipeline());
        pipeline->setShaderStages({{QRhiShaderStage::Vertex, vertex, variant},
                                   {QRhiShaderStage::Fragment, fragment}});
        pipeline->setVertexInputLayout(layout);
        pipeline->setShaderResourceBindings(bindings.get());
        pipeline->setRenderPassDescriptor(target->renderPassDescriptor());
        pipeline->setSampleCount(target->sampleCount());
        QRhiGraphicsPipeline::TargetBlend blend;
        blend.enable = true;
        blend.srcColor = blend.srcAlpha = QRhiGraphicsPipeline::One;
        blend.dstColor = blend.dstAlpha = QRhiGraphicsPipeline::OneMinusSrcAlpha;
        pipeline->setTargetBlends({blend});
        pipeline->setDepthTest(variant == QShader::BatchableVertexShader);
        pipeline->setDepthOp(QRhiGraphicsPipeline::Less);
        if (!pipeline->create()) return false;
    }
    return true;
}

bool SnakeMaterial::supportsWindow(QQuickWindow *window)
{
    if (!window) return false;
    const auto *interface = window->rendererInterface();
    auto *rhi = static_cast<QRhi *>(interface->getResource(window, QSGRendererInterface::RhiResource));
    if (!rhi) return false;
    QSGRendererInterface::GraphicsApi api;
    switch (rhi->backend()) {
    case QRhi::OpenGLES2: api = QSGRendererInterface::OpenGL; break;
    case QRhi::Vulkan: api = QSGRendererInterface::Vulkan; break;
    case QRhi::D3D11: api = QSGRendererInterface::Direct3D11; break;
    case QRhi::D3D12: api = QSGRendererInterface::Direct3D12; break;
    case QRhi::Metal: api = QSGRendererInterface::Metal; break;
    default: return false;
    }
    QSurfaceFormat format;
    if (api == QSGRendererInterface::OpenGL) {
        auto *context = static_cast<QOpenGLContext *>(interface->getResource(window, QSGRendererInterface::OpenGLContextResource));
        if (!context) return false;
        format = context->format();
        format.setRenderableType(context->isOpenGLES() ? QSurfaceFormat::OpenGLES : QSurfaceFormat::OpenGL);
    }
    const auto &pack = shaderPack();
    if (!shadersSupported(pack.vertex, pack.fragment, api, format)) {
        qWarning("Snakes: shader variants unavailable for this backend; using classic rendering");
        return false;
    }
    auto *target = static_cast<QRhiRenderTarget *>(interface->getResource(window, QSGRendererInterface::RhiRedirectRenderTarget));
    if (!target) {
        auto *swapchain = static_cast<QRhiSwapChain *>(interface->getResource(window, QSGRendererInterface::RhiSwapchainResource));
        if (swapchain) target = swapchain->currentFrameRenderTarget();
    }
    if (!probePipelines(rhi, target, pack.vertex, pack.fragment)) {
        qWarning("Snakes: shader pipeline probe failed; using classic rendering");
        return false;
    }
    return true;
}

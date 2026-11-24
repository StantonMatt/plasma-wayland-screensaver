// SPDX-License-Identifier: GPL-3.0-or-later
#include "shore.h"
#include <QFile>
#include <QMutex>
#include <QMutexLocker>
#include <QQuickWindow>
#include <QSGMaterialShader>
#include <algorithm>
#include <cstring>
#include <vector>

namespace {
QShader load(const QString& stage)
{
    QFile f(QStringLiteral(":/depth/aurora/shore.") + stage + QStringLiteral(".qsb"));
    return f.open(QIODevice::ReadOnly) ? QShader::fromSerialized(f.readAll()) : QShader {};
}
const QShader& vertex()
{
    static const auto s = load(QStringLiteral("vert"));
    return s;
}
const QShader& fragment()
{
    static const auto s = load(QStringLiteral("frag"));
    return s;
}

class Material final : public QSGMaterial {
public:
    Material(QSGTexture* light, QSGTexture* starField, QSGTexture* land, uint32_t columns, const aurora_atmosphere& a)
        : emission(light)
        , stars(starField)
        , horizon(land)
        , sky(a)
        , width(float(columns))
    {
        setFlag(RequiresFullMatrix);
        setFlag(NoBatching);
    }
    QSGMaterialType* type() const override
    {
        static QSGMaterialType t;
        return &t;
    }
    QSGMaterialShader* createShader(QSGRendererInterface::RenderMode) const override;
    QSGTexture *emission, *stars, *horizon;
    aurora_atmosphere sky;
    float width;
};
class Shader final : public QSGMaterialShader {
public:
    Shader()
    {
        setShader(VertexStage, vertex());
        setShader(FragmentStage, fragment());
    }
    bool updateUniformData(RenderState& s, QSGMaterial* m, QSGMaterial*) override
    {
        const auto* material = static_cast<const Material*>(m);
        char* data = s.uniformData()->data();
        if (s.isMatrixDirty())
            std::memcpy(data, s.combinedMatrix().constData(), 64);
        const auto& sky = material->sky;
        std::memcpy(data + 64, sky.bounds, 16);
        std::memcpy(data + 80, sky.view, 16);
        std::memcpy(data + 96, sky.top, 16);
        std::memcpy(data + 112, sky.bottom, 16);
        std::memcpy(data + 128, sky.colors[1], 16);
        std::memcpy(data + 144, sky.colors[3], 16);
        const float display[] = { s.opacity(), material->width, sky.clock[3], sky.style[2] == 5.f ? .8f : 1.f };
        std::memcpy(data + 160, display, 16);
        const int palette = int(sky.style[2]);
        const int lightestRole = palette == 0 ? 5 : palette == 5 ? 1 : 0;
        const auto* lightest = sky.colors[lightestRole];
        float limit[] = { 0.95f * lightest[0], 0.95f * lightest[1], 0.95f * lightest[2], sky.phases[1] };
        if (palette == 1) {
            // Spectrum contains both a magenta fringe and high green emission.
            for (int channel = 0; channel < 3; ++channel) {
                for (const auto& role : sky.colors)
                    limit[channel] = std::max(limit[channel], 0.95f * role[channel]);
            }
        }
        std::memcpy(data + 176, limit, 16);
        float optics[4];
        std::memcpy(optics, sky.optics, 16);
        // Preserve faint striation in the crown's diffuse islands. The central
        // convergence aperture remains protected by the shader's canopy rule.
        optics[3] *= 1.f - .65f * sky.phases[2];
        std::memcpy(data + 192, optics, 16);
        return true;
    }
    void updateSampledImage(
        RenderState& s, int binding, QSGTexture** texture, QSGMaterial* m, QSGMaterial*) override
    {
        auto* material = static_cast<Material*>(m);
        if (binding == 1)
            *texture = material->emission;
        else if (binding == 2)
            *texture = material->horizon;
        else if (binding == 3)
            *texture = material->stars;
        else
            return;
        (*texture)->commitTextureOperations(s.rhi(), s.resourceUpdateBatch());
    }
};
QSGMaterialShader* Material::createShader(QSGRendererInterface::RenderMode) const
{
    return new Shader;
}
}
AuroraShore::AuroraShore(QSGTexture* emission, QSGTexture* stars, QSGTexture* horizon, uint32_t columns,
    const aurora_atmosphere& sky, QSizeF logical, float guard)
{
    auto* geometry = new QSGGeometry(QSGGeometry::defaultAttributes_TexturedPoint2D(), 4);
    geometry->setDrawingMode(QSGGeometry::DrawTriangleStrip);
    const qreal width = logical.width() + 2 * guard;
    QSGGeometry::updateTexturedRectGeometry(geometry, QRectF(QPointF(), logical),
        QRectF(guard / width, 0, logical.width() / width, 1));
    setGeometry(geometry);
    setFlag(OwnsGeometry);
    setMaterial(new Material(emission, stars, horizon, columns, sky));
    setFlag(OwnsMaterial);
}
bool AuroraShore::supports(QQuickWindow* window, QRhi* rhi, QRhiBuffer* uniforms, QRhiTexture* emission,
    QRhiTexture* stars, QRhiTexture* horizon, QRhiSampler* sampler)
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
    // All windows on the same adapter and compatible swapchain use this
    // fixed shader/layout pair. Probe it once, then keep the actual per-window
    // resource and emission-pipeline creation checks in AuroraNode.
    struct ProbeKey {
        QRhi::Implementation backend;
        QByteArray device;
        quint64 vendorId, deviceId;
        QVector<quint32> passFormat;
        int samples;
        bool operator==(const ProbeKey&) const = default;
    };
    const auto driver = rhi->driverInfo();
    const ProbeKey key { rhi->backend(), driver.deviceName, driver.vendorId, driver.deviceId,
        target->renderPassDescriptor()->serializedFormat(), target->sampleCount() };
    static QMutex mutex;
    static std::vector<ProbeKey> proven;
    QMutexLocker lock(&mutex);
    if (std::find(proven.cbegin(), proven.cend(), key) != proven.cend())
        return true;
    std::unique_ptr<QRhiShaderResourceBindings> bindings(rhi->newShaderResourceBindings());
    bindings->setBindings({
        QRhiShaderResourceBinding::uniformBuffer(0,
            QRhiShaderResourceBinding::VertexStage | QRhiShaderResourceBinding::FragmentStage, uniforms),
        QRhiShaderResourceBinding::sampledTexture(1, QRhiShaderResourceBinding::FragmentStage, emission, sampler),
        QRhiShaderResourceBinding::sampledTexture(2, QRhiShaderResourceBinding::FragmentStage, horizon, sampler),
        QRhiShaderResourceBinding::sampledTexture(3, QRhiShaderResourceBinding::FragmentStage, stars, sampler),
    });
    if (!bindings->create())
        return false;
    QRhiVertexInputLayout layout;
    layout.setBindings({ { 16 } });
    layout.setAttributes(
        { { 0, 0, QRhiVertexInputAttribute::Float2, 0 }, { 0, 1, QRhiVertexInputAttribute::Float2, 8 } });
    std::unique_ptr<QRhiGraphicsPipeline> pipe(rhi->newGraphicsPipeline());
    pipe->setShaderStages({ { QRhiShaderStage::Vertex, vertex() }, { QRhiShaderStage::Fragment, fragment() } });
    pipe->setVertexInputLayout(layout);
    pipe->setShaderResourceBindings(bindings.get());
    pipe->setRenderPassDescriptor(target->renderPassDescriptor());
    pipe->setSampleCount(target->sampleCount());
    pipe->setTopology(QRhiGraphicsPipeline::TriangleStrip);
    if (!pipe->create())
        return false;
    proven.push_back(key);
    return true;
}

void AuroraShore::sync(const aurora_atmosphere& sky)
{
    static_cast<Material*>(material())->sky = sky;
    markDirty(DirtyMaterial);
}

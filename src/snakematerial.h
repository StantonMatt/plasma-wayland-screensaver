// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <QSGMaterial>
#include <QSGGeometry>
#include <QSurfaceFormat>

class QQuickWindow;
class QShader;
class QRhi;
class QRhiRenderTarget;
class QSGTexture;

class SnakeMaterial final : public QSGMaterial
{
public:
    static constexpr int UniformBinding = 0;
    static constexpr int IconAtlasBinding = 1;
    // std140 layout shared by both baked stages. RHI buffers round up to 16 bytes.
    struct UniformData {
        float matrix[16];
        float opacity;
        float time;
        float light[2];
        float animationTime;
        float motionScale;
        float paletteMode;
        float ambient;
        float season;
    };
    static constexpr int UniformBufferSize = (sizeof(UniformData) + 15) & ~15;
    SnakeMaterial();
    ~SnakeMaterial() override;
    QSGTexture *iconAtlas() const { return m_iconAtlas; }
    QSGMaterialType *type() const override;
    QSGMaterialShader *createShader(QSGRendererInterface::RenderMode) const override;
    int compare(const QSGMaterial *other) const override;
    static const QSGGeometry::AttributeSet &attributes();
    static bool shadersAvailable();
    // Called only on the scene-graph thread, once per newly created node.
    static bool supportsWindow(QQuickWindow *window);
    float time = 0;
    float animationTime = 0;
    float motionScale = 1;
    float paletteMode = 0;
    float ambient = 1;
    float season = 0;
private:
    QSGTexture *m_iconAtlas = nullptr;
    friend class SnakeRendererTest;
    static bool shadersSupported(const QShader &vertex, const QShader &fragment,
                                 QSGRendererInterface::GraphicsApi api,
                                 const QSurfaceFormat &format = {});
    static bool probePipelines(QRhi *rhi, QRhiRenderTarget *target,
                               const QShader &vertex, const QShader &fragment);
};

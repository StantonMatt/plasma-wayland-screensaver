// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include "visuals_core.h"
#include <QSGMaterial>
#include <QSGTexture>
#include <QByteArray>
#include <memory>

class QQuickWindow;
class QRhiTexture;
// Per-node, per-QRhi mutable float data. No atlas, mipmaps, sRGB or filtering.
class VisualDataTexture final : public QSGTexture
{
public:
    VisualDataTexture();
    ~VisualDataTexture() override;
    void setData(const QByteArray &rgba32f, QSize size, QSize uploadSize = {});
    qint64 comparisonKey() const override;
    QRhiTexture *rhiTexture() const override;
    QSize textureSize() const override { return m_size; }
    bool hasAlphaChannel() const override { return false; }
    bool hasMipmaps() const override { return false; }
    void commitTextureOperations(QRhi *, QRhiResourceUpdateBatch *) override;
private:
    std::unique_ptr<QRhiTexture> m_texture;
    QByteArray m_data;
    QSize m_size, m_uploadSize;
    bool m_dirty = false;
};

class VisualMaterial final : public QSGMaterial
{
public:
    enum Technique { Aurora, VertexTextureProof, PrimitiveTexture, PrimitivePlain };
    explicit VisualMaterial(Technique technique = Aurora);
    ~VisualMaterial() override;
    QSGMaterialType *type() const override;
    QSGMaterialShader *createShader(QSGRendererInterface::RenderMode) const override;
    int compare(const QSGMaterial *) const override;
    static bool supportsWindow(QQuickWindow *, Technique = Aurora, int recordCapacity = VISUALS_CORE_MAX_PRIMITIVES);
    static bool shadersAvailable(Technique = Aurora);
    Technique technique;
    visuals_core_snapshot snapshot{};
    VisualDataTexture *dataTexture = nullptr;
    uint32_t primitiveCount = 0;
    int recordCapacity = 0;
    float fringe = 1;
    QSizeF primitiveViewport;
    // std140: matrix, opacity/count/viewport, nine 3-vec4 bands, shade.
    static constexpr int UniformSize = 64 + 16 + 9 * 48 + 16;
};

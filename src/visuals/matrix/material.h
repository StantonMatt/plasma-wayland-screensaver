// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include "visualmaterial.h"
#include "visuals_depth_matrix.h"
#include <rhi/qrhi.h>
#include <vector>

class RainFieldTexture final : public QSGTexture
{
public:
    RainFieldTexture();
    void sync(const vd_rain_layer&);
    qint64 comparisonKey() const override { return qint64(quintptr(this)); }
    QRhiTexture* rhiTexture() const override { return m_texture.get(); }
    QSize textureSize() const override { return m_size; }
    bool hasAlphaChannel() const override { return false; }
    bool hasMipmaps() const override { return false; }
    void commitTextureOperations(QRhi*, QRhiResourceUpdateBatch*) override;

private:
    QSize m_size;
    std::unique_ptr<QRhiTexture> m_texture;
    std::vector<QRhiTextureUploadEntry> m_changes;
    uint32_t m_stamp = 0;
};
class RainMaterial final : public QSGMaterial
{
public:
    explicit RainMaterial(bool heads);
    QSGMaterialType* type() const override;
    QSGMaterialShader* createShader(QSGRendererInterface::RenderMode) const override;
    int compare(const QSGMaterial*) const override;
    static bool supported(QQuickWindow*, QSize atlas);
    bool heads;
    QSGTexture *data = nullptr, *atlas = nullptr;
    vd_rain_uniforms uniforms {};
};

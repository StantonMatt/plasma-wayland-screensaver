// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include "visualmaterial.h"
#include <QSGMaterial>
#include <QImage>
#include <array>
class MatrixAtlasTexture final : public QSGTexture {
public:
    explicit MatrixAtlasTexture(const QImage &alpha);
    ~MatrixAtlasTexture() override;
    qint64 comparisonKey() const override;
    QRhiTexture *rhiTexture() const override;
    QSize textureSize() const override {return m_size;}
    bool hasAlphaChannel() const override {return true;}
    bool hasMipmaps() const override {return true;}
    void commitTextureOperations(QRhi *,QRhiResourceUpdateBatch *) override;
private:
    std::unique_ptr<QRhiTexture> m_texture;
    QImage m_pending;
    QSize m_size;
};
// Static single-channel coverage atlas + one mutable RGBA32F texel per column.
class MatrixMaterial final : public QSGMaterial {
public:
    explicit MatrixMaterial(bool plain=false);
    ~MatrixMaterial() override;
    QSGMaterialType *type() const override;
    QSGMaterialShader *createShader(QSGRendererInterface::RenderMode) const override;
    int compare(const QSGMaterial *) const override;
    static bool supportsWindow(QQuickWindow *,int columns,bool plain=false,QSize atlasSize={16,16});
    static bool shadersAvailable(bool plain=false);
    bool plain;
    float opacity=1;
    int columns=0;
    VisualDataTexture offsets;
    QSGTexture *atlas=nullptr;
};
struct MatrixVertex {
    float position[2], local[4], color[4], column, reserved;
};
static_assert(sizeof(MatrixVertex)==48);

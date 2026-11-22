// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <QSGGeometryNode>
#include <QSGMaterial>
#include <QSGMaterialShader>
#include <QSGTexture>
#include <cstring>

namespace Ribbons {
class DissolveMaterial final : public QSGMaterial {
public:
    QSGTexture* oldStill;
    QSGTexture* newStill;
    float fraction = 0;
    DissolveMaterial(QSGTexture* a, QSGTexture* b) : oldStill(a), newStill(b)
    {
        setFlag(Blending);
        setFlag(NoBatching);
    }
    QSGMaterialType* type() const override
    {
        static QSGMaterialType type;
        return &type;
    }
    QSGMaterialShader* createShader(QSGRendererInterface::RenderMode) const override;
};

class DissolveShader final : public QSGMaterialShader {
public:
    DissolveShader()
    {
        setShaderFileName(VertexStage, QStringLiteral(":/ribbons/dissolve.vert.qsb"));
        setShaderFileName(FragmentStage, QStringLiteral(":/ribbons/dissolve.frag.qsb"));
    }
    bool updateUniformData(RenderState& state, QSGMaterial* material, QSGMaterial*) override
    {
        const auto* silk = static_cast<DissolveMaterial*>(material);
        auto* data = state.uniformData()->data();
        if (state.isMatrixDirty())
            std::memcpy(data, state.combinedMatrix().constData(), 64);
        const float blend[] = { silk->fraction, state.opacity(), 0, 0 };
        std::memcpy(data + 64, blend, sizeof(blend));
        return true;
    }
    void updateSampledImage(RenderState&, int binding, QSGTexture** texture, QSGMaterial* material, QSGMaterial*) override
    {
        const auto* silk = static_cast<DissolveMaterial*>(material);
        *texture = binding == 1 ? silk->oldStill : silk->newStill;
    }
};
inline QSGMaterialShader* DissolveMaterial::createShader(QSGRendererInterface::RenderMode) const
{
    return new DissolveShader;
}

class DissolveNode final : public QSGGeometryNode {
public:
    DissolveNode(QSGTexture* a, QSGTexture* b, QSizeF size)
    {
        auto* geo = new QSGGeometry(QSGGeometry::defaultAttributes_TexturedPoint2D(), 4);
        QSGGeometry::updateTexturedRectGeometry(geo, QRectF(QPointF(), size), QRectF(0,0,1,1));
        setGeometry(geo);
        setFlag(OwnsGeometry);
        setMaterial(new DissolveMaterial(a, b));
        setFlag(OwnsMaterial);
    }
    void setFraction(float fraction)
    {
        static_cast<DissolveMaterial*>(material())->fraction = fraction;
        markDirty(DirtyMaterial);
    }
};
}

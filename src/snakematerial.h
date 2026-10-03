// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <QSGMaterial>
#include <QSGGeometry>
#include <QSurfaceFormat>

class QQuickWindow;
class QShader;
class QRhi;
class QRhiRenderTarget;

class SnakeMaterial final : public QSGMaterial
{
public:
    SnakeMaterial();
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
private:
    friend class SnakeRendererTest;
    static bool shadersSupported(const QShader &vertex, const QShader &fragment,
                                 QSGRendererInterface::GraphicsApi api,
                                 const QSurfaceFormat &format = {});
    static bool probePipelines(QRhi *rhi, QRhiRenderTarget *target,
                               const QShader &vertex, const QShader &fragment);
};

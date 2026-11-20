// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <QSGMaterial>
#include <rhi/qshader.h>
class QQuickWindow;
class StrokeMaterial final : public QSGMaterial
{
public:
    StrokeMaterial();
    QSGMaterialType *type() const override;
    QSGMaterialShader *createShader(QSGRendererInterface::RenderMode) const override;
    int compare(const QSGMaterial *) const override;
    static bool supportsWindow(QQuickWindow *);
    static bool shadersAvailable();
    static QShader vertexShader();
    static QShader fragmentShader();
    static constexpr int UniformSize=80;
};

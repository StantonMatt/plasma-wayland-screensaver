// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include "visuals_depth_kaleidoscope.h"
#include <QSGGeometryNode>
#include <QSGTexture>
#include <rhi/qrhi.h>
#include <rhi/qshader.h>

class QQuickWindow;
QShader kaleidoscopeShader(const char* name);
class FoldNode final : public QSGGeometryNode
{
public:
    FoldNode(QSGTexture* chamber, QSGTexture* bloom, QSizeF size);
    void sync(const kaleidoscope_lens& lens);
    static bool supports(QQuickWindow*, QRhi*, QRhiShaderResourceBindings*);
};

// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include "visuals_depth_aurora.h"
#include <QSGGeometryNode>
#include <QSGMaterial>
#include <QSGTexture>
#include <rhi/qrhi.h>

class AuroraShore final : public QSGGeometryNode {
public:
    AuroraShore(QSGTexture* emission, QSGTexture* stars, QSGTexture* horizon, uint32_t columns,
        const aurora_atmosphere& sky, QSizeF logical, float guard);
    void sync(const aurora_atmosphere& sky);
    // Verify the native composite on this window's actual target, including its sample count.
    static bool supports(QQuickWindow*, QRhi*, QRhiBuffer*, QRhiTexture* emission, QRhiTexture* stars,
        QRhiTexture* horizon, QRhiSampler*);
};

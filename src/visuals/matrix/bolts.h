// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include "visuals_depth_matrix.h"
#include <QSGRenderNode>
#include <memory>
#include <span>
class QQuickWindow;
class QSGTexture;

// Union channel radiance privately; the transparent display adds it to rain.
class RainBolts final : public QSGRenderNode
{
public:
    RainBolts(QQuickWindow*, QSGTexture* atlas);
    ~RainBolts() override;
    void sync(std::span<const vd_rain_head>, uint64_t revision, const vd_rain_uniforms&);
    void prepare() override;
    void render(const RenderState*) override { }
    RenderingFlags flags() const override { return NoExternalRendering; }
    void releaseResources() override;
private:
    struct State;
    std::unique_ptr<State> d;
};

// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include "visuals_depth_starfield.h"
#include "visualworld.h"
#include <QSGRenderNode>
#include <array>
#include <memory>
#include <rhi/qrhi.h>

class LivingVisualItem;
class Windshield final : public QSGRenderNode
{
public:
    Windshield(LivingVisualItem*, QRhi*);
    void sync(const VisualSampleLease&, const vd_sample_info&);
    void prepare() override;
    void render(const RenderState*) override;
    QRectF rect() const override { return m_rect; }
    RenderingFlags flags() const override { return BoundedRectRendering | NoExternalRendering; }
    StateFlags changedStates() const override { return ViewportState | ScissorState | StencilState | BlendState; }
    void releaseResources() override;

private:
    bool createResources();
    LivingVisualItem* m_item;
    QRhi* m_rhi;
    QRectF m_rect;
    VisualSampleLease m_sample;
    vd_star_packet m_packet {};
    std::array<uint64_t, VD_STAR_ROWS> m_resident {};
    std::unique_ptr<QRhiTexture> m_stars, m_noise;
    std::unique_ptr<QRhiSampler> m_nearest, m_linear;
    std::unique_ptr<QRhiBuffer> m_block;
    std::unique_ptr<QRhiShaderResourceBindings> m_bindings;
    std::unique_ptr<QRhiGraphicsPipeline> m_pipeline;
    QRhiRenderPassDescriptor* m_pass = nullptr;
    bool m_failed = false;
};

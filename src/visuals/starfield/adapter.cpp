// SPDX-License-Identifier: GPL-3.0-or-later
#include "livingvisualitem.h"
#include "visualdepthregistry.h"
#include "windshield.h"
#include <QQuickWindow>
#include <rhi/qrhi.h>

static void initializeStarShaders() { Q_INIT_RESOURCE(depth_starfield_shaders); }

namespace
{
class StarfieldItem final : public LivingVisualItem
{
public:
    using LivingVisualItem::LivingVisualItem;

protected:
    QSGNode* buildDepthNode(QSGNode* old, const VisualSampleLease& sample, const vd_sample_info& info,
        const vd_endpoint&, const vd_endpoint&) override
    {
        auto* node = static_cast<Windshield*>(old);
        if (!node) {
            auto* rhi = window()->rhi();
            if (!rhi || rhi->backend() == QRhi::Null || !rhi->isFeatureSupported(QRhi::Instancing)
                || !rhi->isTextureFormatSupported(QRhiTexture::RGBA32F)
                || rhi->resourceLimit(QRhi::TextureSizeMax) < 2 * VD_STAR_WIDTH) {
                Q_EMIT gpuUnavailable();
                return nullptr;
            }
            node = new Windshield(this, rhi);
        }
        node->sync(sample, info);
        return node;
    }
};
}
void registerDepthStarfieldAdapter()
{
    initializeStarShaders();
    VisualDepthRegistry::registerItemFactory(3, [](QQuickItem* parent) { return new StarfieldItem(parent); });
}

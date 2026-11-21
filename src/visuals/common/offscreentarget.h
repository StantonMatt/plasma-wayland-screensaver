// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <QQuickWindow>
#include <QSGTexture>
#include <memory>
#include <rhi/qrhi.h>

// One render-thread colour attachment, with a scene-graph texture for display.
// Consumers own their pipelines and record draws between beginPass/endPass.
// Destroy pipelines and detach display nodes before destroying this target.
class OffscreenTarget final {
public:
    static std::unique_ptr<OffscreenTarget> create(
        QQuickWindow* window, QRhi* rhi, QSize size, QQuickWindow::CreateTextureOptions textureOptions = {})
    {
        if (!window || !rhi || size.isEmpty())
            return {};
        auto attachment = std::unique_ptr<QRhiTexture>(
            rhi->newTexture(QRhiTexture::RGBA8, size, 1, QRhiTexture::RenderTarget));
        if (!attachment->create())
            return {};
        auto result = std::unique_ptr<OffscreenTarget>(new OffscreenTarget);
        result->m_target.reset(rhi->newTextureRenderTarget(
            QRhiTextureRenderTargetDescription { QRhiColorAttachment(attachment.get()) }));
        result->m_pass.reset(result->m_target->newCompatibleRenderPassDescriptor());
        result->m_target->setRenderPassDescriptor(result->m_pass.get());
        if (!result->m_target->create())
            return {};
        result->m_texture.reset(window->createTextureFromRhiTexture(attachment.get(), textureOptions));
        if (!result->m_texture)
            return {};
        attachment.release(); // QSGTexture owns the QRhiTexture now.
        return result;
    }

    static QRhiGraphicsPipeline::TargetBlend screenBlend()
    {
        QRhiGraphicsPipeline::TargetBlend blend;
        blend.enable = true;
        blend.srcColor = QRhiGraphicsPipeline::One;
        blend.dstColor = QRhiGraphicsPipeline::OneMinusSrcColor;
        blend.srcAlpha = QRhiGraphicsPipeline::One;
        blend.dstAlpha = QRhiGraphicsPipeline::OneMinusSrcAlpha;
        return blend;
    }

    QRhiTextureRenderTarget* target() const { return m_target.get(); }
    QSGTexture* texture() const { return m_texture.get(); }

private:
    OffscreenTarget() = default;
    std::unique_ptr<QSGTexture> m_texture;
    std::unique_ptr<QRhiRenderPassDescriptor> m_pass;
    std::unique_ptr<QRhiTextureRenderTarget> m_target;
};

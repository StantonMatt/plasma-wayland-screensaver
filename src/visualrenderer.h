// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include "visuals_core.h"
#include <QQuickItem>
#include <memory>
#include <optional>

// Qt owns scheduling/resources; Rust owns settings interpretation and visual math.
// Future techniques share this item and immutable snapshot handoff.
class VisualRenderer final : public QQuickItem
{
    Q_OBJECT
    friend class VisualRendererTest;
    friend class OverlaySnakesTest;
public:
    explicit VisualRenderer(QQuickItem *parent=nullptr);
    Q_INVOKABLE void configureAurora(int density, qreal scale, qreal speed, qreal trails,
        const QString &palette, int seed, bool reducedMotion, double epochMs);
    Q_INVOKABLE void setViewport(const QRectF &worldBounds, const QPointF &origin);
    Q_INVOKABLE void presentAt(qint64 presentationNanoseconds);
protected:
    QSGNode *updatePaintNode(QSGNode *, UpdatePaintNodeData *) override;
    void geometryChange(const QRectF &,const QRectF &) override;
private:
    void captureSnapshot(bool advance);
    std::unique_ptr<visuals_core,decltype(&visuals_core_destroy)> m_core{nullptr,visuals_core_destroy};
    visuals_core_config m_config{};
    visuals_core_snapshot m_snapshot{};
    QRectF m_worldBounds;
    QPointF m_viewportOrigin;
    std::optional<double> m_epochMs;
    qint64 m_presentationNs=0;
    bool m_hasPresentation=false;
    bool m_shaderInUse=false;
    bool m_shaderFailed=false;
    QMetaObject::Connection m_errorConnection;
    bool (*m_supportCheck)(QQuickWindow *)=nullptr;
};

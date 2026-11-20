// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include "visuals_strokes.h"
#include <QQuickItem>
#include <memory>
#include <optional>
#include <vector>
// Thin Qt scheduling/resource glue; all paths, palette and geometry live in Rust.
class StrokeRenderer final : public QQuickItem
{
    Q_OBJECT
    friend class StrokeRendererTest;
    friend class OverlaySnakesTest;
public:
    explicit StrokeRenderer(QQuickItem *parent=nullptr);
    Q_INVOKABLE void configureStrokes(const QString &visual,int density,qreal scale,qreal speed,qreal trails,
        const QString &palette,int seed,bool reducedMotion,double epochMs);
    Q_INVOKABLE void setViewport(const QRectF &worldBounds,const QPointF &origin);
    Q_INVOKABLE void presentAt(qint64 presentationNanoseconds);
protected:
    QSGNode *updatePaintNode(QSGNode *,UpdatePaintNodeData *) override;
    void geometryChange(const QRectF &,const QRectF &) override;
private:
    void captureSnapshot(bool advance);
    std::unique_ptr<visuals_strokes,decltype(&visuals_strokes_destroy)> m_core{nullptr,visuals_strokes_destroy};
    visuals_core_config m_config{};
    std::vector<visuals_strokes_vertex> m_vertices;
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

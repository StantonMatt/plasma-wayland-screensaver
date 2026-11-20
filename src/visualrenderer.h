// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include "visuals_core.h"
#include "visuals_bounce.h"
#include <functional>
#include <QQuickItem>
#include <memory>
#include <optional>
#include <vector>

// Qt owns scheduling/resources; Rust owns settings interpretation and visual math.
// Future techniques share this item and immutable snapshot handoff.
class VisualRenderer final : public QQuickItem
{
    Q_OBJECT
    friend class VisualRendererTest;
    friend class BounceRendererTest;
    friend class OverlaySnakesTest;
public:
    explicit VisualRenderer(QQuickItem *parent=nullptr);
    Q_INVOKABLE void configureAurora(int density, qreal scale, qreal speed, qreal trails,
        const QString &palette, int seed, bool reducedMotion, double epochMs);
    Q_INVOKABLE void configureConstellation(int density, qreal scale, qreal speed, qreal trails,
        const QString &palette, int seed, bool reducedMotion, double epochMs);
    Q_INVOKABLE void configureStarfield(int density, qreal scale, qreal speed, qreal trails,
        const QString &palette, int seed, bool reducedMotion, double epochMs);
    Q_INVOKABLE void configureBounce(int density, qreal scale, qreal speed, qreal trails,
        const QString &palette, int seed, bool reducedMotion, double epochMs,
        int count, qreal gravity, qreal elasticity, bool collisions, int mode);
    using BounceSource=std::function<size_t(qint64, visuals_bounce_ball *, size_t)>;
    void setBounceSource(BounceSource source) { m_bounceSource=std::move(source); }
    Q_INVOKABLE void setViewport(const QRectF &worldBounds, const QPointF &origin);
    void presentBounceDelta(qreal deltaSeconds);
    Q_INVOKABLE void presentAt(qint64 presentationNanoseconds);
protected:
    QSGNode *updatePaintNode(QSGNode *, UpdatePaintNodeData *) override;
    void geometryChange(const QRectF &,const QRectF &) override;
private:
    void configure(uint32_t visual, int density, qreal scale, qreal speed, qreal trails,
        const QString &palette, int seed, bool reducedMotion, double epochMs);
    void captureSnapshot(bool advance);
    QSGNode *updatePrimitives(QSGNode *);
    std::unique_ptr<visuals_bounce,decltype(&visuals_bounce_destroy)> m_bounce{nullptr,visuals_bounce_destroy};
    visuals_bounce_settings m_bounceSettings{VISUALS_BOUNCE_ABI_VERSION,sizeof(visuals_bounce_settings),1,0,0,100,0,0};
    BounceSource m_bounceSource;
    qreal m_bounceDeltaSeconds=0;
    std::vector<visuals_core_primitive> m_primitives;
    uint32_t m_primitiveCount=0;
    bool m_plainInUse=false;
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

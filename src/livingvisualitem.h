// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include "visualworld.h"
#include <QQuickItem>
#include <atomic>
class LivingVisualItem : public QQuickItem
{
    Q_OBJECT
public:
    explicit LivingVisualItem(QQuickItem* parent = nullptr);
    // GUI only. The Qt synchronization barrier transfers this immutable lease.
    void setSample(VisualSampleLease);
    quint64 renderedSample() const;
    quint64 renderedLayout() const;
    quint64 renderedDataRevision() const;
    qint64 renderedTimelineNs() const;

protected:
    // The base binds time/view/generation once and acknowledges afterRendering.
    // Owned adapters receive immutable endpoints, never the mutable world.
    QSGNode* updatePaintNode(QSGNode*, UpdatePaintNodeData*) final;
    virtual QSGNode* buildDepthNode(
        QSGNode*, const VisualSampleLease&, const vd_sample_info&, const vd_endpoint& lower, const vd_endpoint& upper);

private:
    VisualSampleLease m_pending;
    struct RenderProgress
    {
        std::atomic<quint64> synchronized { 0 }, rendered { 0 };
        std::atomic<qint64> timeline { -1 }, renderedTimeline { -1 };
        std::atomic<quint64> layout { 0 }, data { 0 }, renderedLayout { 0 }, renderedData { 0 };
    };
    std::shared_ptr<RenderProgress> m_progress;
    QMetaObject::Connection m_renderConnection;
};

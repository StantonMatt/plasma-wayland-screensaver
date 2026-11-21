// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include "visuals_depth.h"
#include <QPointF>
#include <QThread>
#include <memory>
#include <vector>

using VisualSampleLease = std::shared_ptr<const visuals_depth_sample>;
// Exclusive mutable Rust owner. May be constructed/pre-rolled on a worker, then
// transferred before publication; every public runtime call stays on its owner.
class VisualWorld final
{
public:
    VisualWorld(const vd_config&, const std::vector<vd_value>&, const std::vector<vd_output>&, const vd_environment&,
        const std::vector<vd_obstacle>&, quint64 generation, qint64 leadNs);
    ~VisualWorld();
    VisualWorld(const VisualWorld&) = delete;
    VisualWorld& operator=(const VisualWorld&) = delete;
    bool valid() const
    {
        return m_world != nullptr;
    }
    bool ensureThrough(qint64 timelineNs, uint32_t maxTicks = 8);
    VisualSampleLease sample(const vd_view&);
    vd_stats statistics() const;
    // Ownership may transfer only before the object is published to a coordinator.
    void adoptOnCurrentThread();
#ifdef PVS_ENABLE_VISUAL_CAPTURE
    bool seek(qint64 timelineNs);
    bool inject(uint32_t event, qint64 timelineNs, QPointF anchor, double strength);
    quint64 stateHash() const;
#endif
private:
    void assertOwner() const;
    visuals_depth_world* m_world = nullptr;
    QThread* m_owner = nullptr;
    // Slots are mutable only while the pool is their sole owner. Readers keep
    // the immutable handle and endpoints alive, including after world retirement.
    std::vector<std::shared_ptr<visuals_depth_sample>> m_samples;
};

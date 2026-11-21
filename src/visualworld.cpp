// SPDX-License-Identifier: GPL-3.0-or-later
#include "visualworld.h"
#include "visuals_depth_debug.h"
#include <QPointF>
// Internal staticlib helper: the caller must exclusively own the sample slot.
extern "C" int32_t visuals_depth_update_sample(visuals_depth_world*, const vd_view*, visuals_depth_sample*);
VisualWorld::VisualWorld(const vd_config& config, const std::vector<vd_value>& values,
    const std::vector<vd_output>& outputs, const vd_environment& environment, const std::vector<vd_obstacle>& obstacles,
    quint64 generation, qint64 leadNs)
    : m_owner(QThread::currentThread())
{
    visuals_depth_create(&config, values.data(), values.size(), outputs.data(), outputs.size(), &environment,
        obstacles.data(), obstacles.size(), generation, leadNs, &m_world);
}
VisualWorld::~VisualWorld()
{
    visuals_depth_destroy(m_world);
}
void VisualWorld::assertOwner() const
{
    Q_ASSERT(QThread::currentThread() == m_owner);
}
void VisualWorld::adoptOnCurrentThread()
{
    m_owner = QThread::currentThread();
}
bool VisualWorld::ensureThrough(qint64 ns, uint32_t maxTicks)
{
    assertOwner();
    return visuals_depth_ensure_through(m_world, ns, maxTicks) == 0;
}
VisualSampleLease VisualWorld::sample(const vd_view& view)
{
    assertOwner();
    for (const auto& slot : m_samples)
        if (slot.use_count() == 1)
        {
            if (visuals_depth_update_sample(m_world, &view, slot.get()) != 0)
                return {};
            return slot;
        }
    visuals_depth_sample* lease = nullptr;
    if (visuals_depth_retain_sample(m_world, &view, &lease) != 0)
        return {};
    m_samples.emplace_back(lease,
        [](const visuals_depth_sample* s) { visuals_depth_sample_release(const_cast<visuals_depth_sample*>(s)); });
    return m_samples.back();
}
vd_stats VisualWorld::statistics() const
{
    assertOwner();
    vd_stats stats {};
    visuals_depth_stats(m_world, &stats);
    return stats;
}
#ifdef PVS_ENABLE_VISUAL_CAPTURE
bool VisualWorld::seek(qint64 ns)
{
    assertOwner();
    return visuals_depth_debug_seek(m_world, ns) == 0;
}
bool VisualWorld::inject(uint32_t event, qint64 ns, QPointF p, double strength)
{
    assertOwner();
    vd_event value { event, ns, { p.x(), p.y() }, strength };
    return visuals_depth_debug_inject(m_world, &value) == 0;
}
quint64 VisualWorld::stateHash() const
{
    assertOwner();
    return visuals_depth_debug_hash(m_world);
}
#endif

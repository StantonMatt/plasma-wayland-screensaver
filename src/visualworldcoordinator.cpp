// SPDX-License-Identifier: GPL-3.0-or-later
#include "visualworldcoordinator.h"
#include "livingvisualitem.h"
#include <QCoreApplication>
#include <algorithm>
#include <chrono>
#include <cmath>
namespace
{
quint64 hash64(quint64 x)
{
    x += 0x9e3779b97f4a7c15ULL;
    x = (x ^ (x >> 30)) * 0xbf58476d1ce4e5b9ULL;
    x = (x ^ (x >> 27)) * 0x94d049bb133111ebULL;
    return x ^ (x >> 31);
}
vd_rect rect(QRectF r)
{
    return { r.x(), r.y(), r.width(), r.height() };
}
}
VisualWorldCoordinator::VisualWorldCoordinator(QObject* parent)
    : QObject(parent)
{
    m_wake.setSingleShot(true);
    m_wake.setTimerType(Qt::PreciseTimer);
    connect(&m_wake, &QTimer::timeout, this, [this] {
        const auto now = monotonicNow();
        for (auto it = m_bindings.cbegin(); it != m_bindings.cend(); ++it)
            present(it.key(), now);
        scheduleWake(now);
    });
}

void VisualWorldCoordinator::scheduleWake(qint64 now)
{
    m_wake.stop();
    const qint64 elapsed = std::max<qint64>(0, now - m_timeOrigin);
    qint64 next = -1;
    for (const auto& world : m_worlds) {
        const auto due = world->nextWake(elapsed);
        if (due > elapsed && (next < 0 || due < next))
            next = due;
    }
    if (next >= 0)
        m_wake.start(std::chrono::milliseconds((next - elapsed + 999999) / 1000000));
}
qint64 VisualWorldCoordinator::monotonicNow()
{
    return std::chrono::duration_cast<std::chrono::nanoseconds>(std::chrono::steady_clock::now().time_since_epoch())
        .count();
}
quint64 VisualWorldCoordinator::outputIdentity(const QString& connector)
{
    quint64 h = 0xcbf29ce484222325ULL;
    for (auto b : connector.toUtf8())
        h = (h ^ quint8(b)) * 0x100000001b3ULL;
    return h;
}
quint64 VisualWorldCoordinator::independentSeed(quint64 base, uint32_t visual, const QString& connector)
{
    return hash64(hash64(hash64(base ^ visual) ^ outputIdentity(connector)));
}
void VisualWorldCoordinator::assertGui() const
{
    Q_ASSERT(QThread::currentThread() == thread());
    Q_ASSERT(!qApp || QThread::currentThread() == qApp->thread());
}
bool VisualWorldCoordinator::configure(const vd_config& input, const std::vector<vd_value>& parameters,
    QVector<VisualOutput> outputs, VisualMonitorMode mode, VisualMirroredPolicy policy,
    const VisualEnvironment& environment, qint64 epochNs, qint64 recreationNs)
{
    assertGui();
    // Keep the last complete layout visible while hotplug configuration settles.
    if (outputs.isEmpty()
        || std::any_of(outputs.cbegin(), outputs.cend(),
            [](const auto& o) { return !o.configured || o.connector.isEmpty() || o.viewport.isEmpty(); }))
        return false;
    std::sort(outputs.begin(), outputs.end(), [](const auto& a, const auto& b) { return a.connector < b.connector; });
    for (qsizetype i = 1; i < outputs.size(); ++i)
        if (outputs[i - 1].connector == outputs[i].connector)
            return false;
    // A replacement runs its normal bounded startup at the current logical
    // time, rather than replaying the entire activation's elapsed time.
    const bool recreating = !m_worlds.empty();
    auto old = std::move(m_bindings);
    m_worlds.clear();
    m_bindings.clear();
    m_epoch = epochNs;
    m_timeOrigin = recreating ? std::max(epochNs, recreationNs) : epochNs;
    // The bridge supplies the longest actual PresentationClock period (frame
    // cap plus refresh divisor). Cover it at the maximum supported 120 Hz,
    // with four extra ticks to drain activation/stall backlog, at most 32.
    // Per-world/per-window budgets can replace this conservative bound once
    // real visuals expose their tick rates.
    m_tickBudget = uint32_t(std::clamp(std::ceil(120.L * environment.presentationLeadNs / 1'000'000'000.L),
                               0.L, 28.L))
        + 4;
    ++m_generation;
    const auto reference = std::max_element(outputs.cbegin(), outputs.cend(),
        [](const auto& a, const auto& b)
        {
            const double aa = a.viewport.width() * a.viewport.height(), ba = b.viewport.width() * b.viewport.height();
            return aa < ba;
        });
    const bool shared = mode == VisualMonitorMode::Spanning
        || (mode == VisualMonitorMode::Mirrored && policy == VisualMirroredPolicy::SharedCanonical);
    const QRectF canonical(QPointF(0, 0), reference->viewport.size());
    for (const auto& output : outputs)
    {
        Binding binding;
        binding.world = shared ? 0 : m_worlds.size();
        binding.view
            = { outputIdentity(output.connector), m_generation, 0, rect(output.viewport), 1., 0., 0., output.dpr };
        if (mode != VisualMonitorMode::Spanning)
            binding.view.viewport = rect(QRectF(QPointF(0, 0), output.viewport.size()));
        if (mode == VisualMonitorMode::Mirrored && shared)
        {
            const auto fit = visuals_depth_mirrored_fit(input.visual);
            const bool contain = fit == 1;
            const double sx = output.viewport.width() / canonical.width(),
                         sy = output.viewport.height() / canonical.height();
            binding.view.uniform_scale = fit == 2 ? sy : contain ? std::min(sx, sy) : std::max(sx, sy);
            binding.view.offset_x = (output.viewport.width() - canonical.width() * binding.view.uniform_scale) / 2.;
            binding.view.offset_y = (output.viewport.height() - canonical.height() * binding.view.uniform_scale) / 2.;
        }
        if (!shared || m_worlds.empty())
        {
            auto config = input;
            config.monitor_mode = uint32_t(mode);
            if (mode == VisualMonitorMode::Independent)
                config.seed = independentSeed(input.seed, input.visual, output.connector);
            std::vector<vd_output> arenas;
            std::vector<vd_obstacle> obstacles;
            const auto addOutput = [&](const VisualOutput& o, QRectF arena)
            {
                arenas.push_back(
                    { outputIdentity(o.connector), rect(arena), o.dpr, o.refreshHz, uint32_t(o.primary),
                      mode == VisualMonitorMode::Independent ? uint32_t(&o - outputs.constData()) : 0u });
            };
            if (mode == VisualMonitorMode::Spanning)
                for (const auto& o : outputs)
                    addOutput(o, o.viewport);
            else if (shared)
                addOutput(*reference, canonical);
            else
                addOutput(output, QRectF(QPointF(0, 0), output.viewport.size()));
            for (const auto& o : outputs)
            {
                const bool mirroredClock = input.visual == 8 && mode == VisualMonitorMode::Mirrored && shared;
                if (!mirroredClock && mode != VisualMonitorMode::Spanning
                    && o.connector != (shared ? reference->connector : output.connector))
                    continue;
                QRectF obstacle = environment.clockRects.value(o.connector);
                if (obstacle.isEmpty())
                    continue;
                if (mode == VisualMonitorMode::Spanning)
                    obstacle.translate(o.viewport.topLeft());
                auto velocity = environment.clockVelocity.value(o.connector);
                if (mode == VisualMonitorMode::Mirrored && shared)
                {
                    const double sx = o.viewport.width() / canonical.width(),
                                 sy = o.viewport.height() / canonical.height();
                    const auto fit = visuals_depth_mirrored_fit(input.visual);
                    const double scale = fit == 2 ? sy : fit == 1 ? std::min(sx, sy) : std::max(sx, sy);
                    const QPointF offset((o.viewport.width() - canonical.width() * scale) / 2.,
                        (o.viewport.height() - canonical.height() * scale) / 2.);
                    if (mirroredClock) {
                        // The local clock bounces inside this visible cover crop.
                        const QRectF bounds(-offset / scale, o.viewport.size() / scale);
                        obstacles.push_back({outputIdentity(o.connector), rect(bounds), 0., 2, 1, 0., 0.});
                        velocity /= scale;
                    }
                    obstacle = QRectF((obstacle.topLeft() - offset) / scale, obstacle.size() / scale);
                }
                bool duplicate = false;
                for (const auto& prior : obstacles)
                    if (prior.rect.x == obstacle.x() && prior.rect.y == obstacle.y()
                        && prior.rect.width == obstacle.width() && prior.rect.height == obstacle.height())
                        duplicate = true;
                if (mirroredClock || !duplicate) {
                    const auto v = velocity;
                    obstacles.push_back({ outputIdentity(o.connector), rect(obstacle), .1, 1, 1, v.x(), v.y() });
                }
            }
            auto world = std::make_unique<VisualWorld>(
                config, parameters, arenas, environment.colours, obstacles, m_generation, environment.presentationLeadNs);
            if (!world->valid())
            {
                stop();
                return false;
            }
            m_worlds.push_back(std::move(world));
        }
        binding.item = old.value(output.connector).item;
        binding.lease = old.value(output.connector).lease;
        m_bindings.insert(output.connector, binding);
    }
    for (auto it = old.cbegin(); it != old.cend(); ++it)
        if (!m_bindings.contains(it.key()) && it->item)
            it->item->setSample({});
    scheduleWake(monotonicNow());
    return true;
}
void VisualWorldCoordinator::attach(const QString& connector, LivingVisualItem* item)
{
    assertGui();
    if (m_bindings.contains(connector))
    {
        m_bindings[connector].item = item;
        if (item)
            item->setSample(m_bindings[connector].lease);
    }
}
bool VisualWorldCoordinator::present(const QString& connector, qint64 ns)
{
    assertGui();
    auto it = m_bindings.find(connector);
    if (it == m_bindings.end())
        return false;
    auto& b = it.value();
    const auto elapsed = std::max<qint64>(0, ns - m_timeOrigin);
    // Bounded catch-up. Even if still behind, publish the latest available
    // endpoints; sampling clamps their timeline rather than starving a window.
    // A delayed output reads history; no other output repaints.
    m_worlds[b.world]->ensureThrough(elapsed, m_tickBudget);
    b.view.timeline_ns = elapsed;
    auto lease = m_worlds[b.world]->sample(b.view);
    if (!lease)
        return false;
    b.lease = std::move(lease);
    if (b.item)
        b.item->setSample(b.lease);
    return bool(b.lease);
}
void VisualWorldCoordinator::stop()
{
    m_wake.stop();
    assertGui();
    for (auto& b : m_bindings)
        if (b.item)
            b.item->setSample({});
    m_bindings.clear();
    m_worlds.clear();
}
qsizetype VisualWorldCoordinator::worldCount() const
{
    assertGui();
    return qsizetype(m_worlds.size());
}
VisualSampleLease VisualWorldCoordinator::sampleFor(const QString& connector) const
{
    assertGui();
    return m_bindings.value(connector).lease;
}
QVector<vd_stats> VisualWorldCoordinator::statistics() const
{
    assertGui();
    QVector<vd_stats> stats;
    for (const auto& w : m_worlds)
        stats.append(w->statistics());
    return stats;
}
#ifdef PVS_ENABLE_VISUAL_CAPTURE
bool VisualWorldCoordinator::seek(qint64 ns)
{
    assertGui();
    for (auto& w : m_worlds)
        if (!w->seek(ns))
            return false;
    return true;
}
bool VisualWorldCoordinator::inject(uint32_t event, qint64 ns, QPointF p, double strength)
{
    assertGui();
    for (auto& w : m_worlds)
        if (!w->inject(event, ns, p, strength))
            return false;
    return true;
}
QVector<quint64> VisualWorldCoordinator::stateHashes() const
{
    assertGui();
    QVector<quint64> hashes;
    for (const auto& w : m_worlds)
        hashes.append(w->stateHash());
    return hashes;
}
#endif

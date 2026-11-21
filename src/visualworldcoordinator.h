// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include "visualworld.h"
#include <QMap>
#include <QObject>
#include <QPointer>
#include <QRectF>
#include <QString>
#include <QVector>
class Configuration;
class LivingVisualItem;
enum class VisualMonitorMode : uint32_t
{
    Independent = 0,
    Mirrored = 1,
    Spanning = 2
};
enum class VisualMirroredPolicy
{
    SharedCanonical,
    PerOutputLocal
};
struct VisualOutput
{
    QString connector;
    QRectF viewport;
    double dpr = 1.;
    double refreshHz = 60.;
    bool primary = false;
    bool configured = false;
};
struct VisualEnvironment
{
    vd_environment colours {};
    qint64 presentationLeadNs = 50'000'000;
    // Actual clock bounds at activation. Mirrored takes only the reference
    // output rectangle and maps it into the canonical arena.
    QMap<QString, QRectF> clockRects;
};
class VisualWorldCoordinator final : public QObject
{
    Q_OBJECT
public:
    explicit VisualWorldCoordinator(QObject* parent = nullptr);
    static qint64 monotonicNow();
    static quint64 outputIdentity(const QString& connector);
    static quint64 independentSeed(quint64 base, uint32_t visual, const QString& connector);
    // Configured means the corresponding layer-shell view has received configure.
    bool configure(const vd_config&, const std::vector<vd_value>&, QVector<VisualOutput>, VisualMonitorMode,
        VisualMirroredPolicy, const VisualEnvironment&, qint64 epochNs, qint64 recreationNs = monotonicNow());
    void attach(const QString& connector, LivingVisualItem*);
    bool present(const QString& connector, qint64 predictedPresentationNs);
    void stop();
    qsizetype worldCount() const;
    quint64 layoutGeneration() const
    {
        return m_generation;
    }
    qint64 epoch() const
    {
        return m_epoch;
    }
    VisualSampleLease sampleFor(const QString& connector) const;
    QVector<vd_stats> statistics() const;
#ifdef PVS_ENABLE_VISUAL_CAPTURE
    bool seek(qint64 ns);
    bool inject(uint32_t event, qint64 ns, QPointF anchor, double strength = 1.);
    QVector<quint64> stateHashes() const;
#endif
private:
    void assertGui() const;
    std::vector<std::unique_ptr<VisualWorld>> m_worlds;
    struct Binding
    {
        size_t world = 0;
        vd_view view {};
        QPointer<LivingVisualItem> item;
        VisualSampleLease lease;
    };
    QMap<QString, Binding> m_bindings;
    quint64 m_generation = 0;
    qint64 m_epoch = 0;
    qint64 m_timeOrigin = 0;
    uint32_t m_tickBudget = 8;
};

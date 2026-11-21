// SPDX-License-Identifier: GPL-3.0-or-later
#include "visualdepthregistry.h"
#include "livingvisualitem.h"
#include <QMap>
#include <QStringList>
#include <cmath>
void registerDepthAuroraAdapter();
void registerDepthConstellationAdapter();
void registerDepthStarfieldAdapter();
void registerDepthRibbonsAdapter();
void registerDepthKaleidoscopeAdapter();
void registerDepthMatrixAdapter();
void registerDepthBounceAdapter();
void registerDepthOrbsAdapter();
void registerDepthFirefliesAdapter();
static void initializeDepthResources()
{
    Q_INIT_RESOURCE(depth_aurora_settings);
    Q_INIT_RESOURCE(depth_constellation_settings);
    Q_INIT_RESOURCE(depth_starfield_settings);
    Q_INIT_RESOURCE(depth_ribbons_settings);
    Q_INIT_RESOURCE(depth_kaleidoscope_settings);
    Q_INIT_RESOURCE(depth_matrix_settings);
    Q_INIT_RESOURCE(depth_bounce_settings);
    Q_INIT_RESOURCE(depth_orbs_settings);
    Q_INIT_RESOURCE(depth_fireflies_settings);
}
static void initializeDepthAdapters()
{
    registerDepthAuroraAdapter();
    registerDepthConstellationAdapter();
    registerDepthStarfieldAdapter();
    registerDepthRibbonsAdapter();
    registerDepthKaleidoscopeAdapter();
    registerDepthMatrixAdapter();
    registerDepthBounceAdapter();
    registerDepthOrbsAdapter();
    registerDepthFirefliesAdapter();
}
namespace
{
struct Registry
{
    QMap<QString, uint32_t> ids;
    QMap<QString, QVariantList> schemas;
    Registry()
    {
        initializeDepthResources();
        // ABI identities, not gallery ordering. Parameter declarations remain local Rust tables.
        const QStringList names = { QStringLiteral("aurora"), QStringLiteral("constellation"),
            QStringLiteral("starfield"), QStringLiteral("ribbons"), QStringLiteral("kaleidoscope"),
            QStringLiteral("matrix"), QStringLiteral("bounce"), QStringLiteral("orbs"), QStringLiteral("fireflies") };
        for (qsizetype i = 0; i < names.size(); ++i)
        {
            const auto id = uint32_t(i + 1);
            ids.insert(names[i], id);
            const vd_parameter* p = nullptr;
            size_t count = 0;
            if (visuals_depth_schema(id, &p, &count) != 0)
                qFatal("Invalid visual depth schema");
            QVariantList entries;
            for (size_t j = 0; j < count; ++j)
            {
                const auto& v = p[j];
                const auto choices = QString::fromUtf8(v.choices).split(QLatin1Char('|'), Qt::SkipEmptyParts);
                QVariant value = v.kind == 2 ? QVariant(bool(v.default_value))
                    : v.kind == 3            ? QVariant(choices.value(int(v.default_value)))
                                             : QVariant(int(v.default_value));
                entries.append(QVariantMap { { QStringLiteral("id"), v.id },
                    { QStringLiteral("key"), QString::fromUtf8(v.key) }, { QStringLiteral("kind"), v.kind },
                    { QStringLiteral("minimum"), v.minimum }, { QStringLiteral("maximum"), v.maximum },
                    { QStringLiteral("default"), value }, { QStringLiteral("choices"), choices } });
            }
            schemas.insert(names[i], entries);
        }
    }
};
#ifdef PVS_ENABLE_VISUAL_CAPTURE
QMap<QString,bool> &availabilityOverrides() {static QMap<QString,bool> values;return values;}
#endif
const Registry& registry()
{
    static const Registry r;
    return r;
}
}
uint32_t VisualDepthRegistry::id(const QString& visual)
{
    return registry().ids.value(visual);
}
bool VisualDepthRegistry::available(const QString& visual)
{
#ifdef PVS_ENABLE_VISUAL_CAPTURE
    if(availabilityOverrides().contains(visual))return availabilityOverrides().value(visual);
#endif
    return visuals_depth_available(id(visual)) != 0;
}
QVariantList VisualDepthRegistry::schema(const QString& visual)
{
    return registry().schemas.value(visual);
}
QVariantMap VisualDepthRegistry::defaults(const QString& visual)
{
    QVariantMap values;
    for (const auto& v : schema(visual))
    {
        const auto p = v.toMap();
        values.insert(p.value(QStringLiteral("key")).toString(), p.value(QStringLiteral("default")));
    }
    return values;
}
QVariant VisualDepthRegistry::normalize(const QVariantMap& p, const QVariant& value)
{
    const auto fallback = p.value(QStringLiteral("default"));
    switch (p.value(QStringLiteral("kind")).toUInt())
    {
    case 2:
        if (value.metaType().id() == QMetaType::Bool)
            return value;
        if (value.toString() == QStringLiteral("true") || value.toString() == QStringLiteral("1"))
            return true;
        if (value.toString() == QStringLiteral("false") || value.toString() == QStringLiteral("0"))
            return false;
        return fallback;
    case 3:
        return p.value(QStringLiteral("choices")).toStringList().contains(value.toString()) ? value.toString()
                                                                                            : fallback;
    default:
    {
        if(value.metaType().id()==QMetaType::Bool)return fallback;
        bool ok = false;
        const double n = value.toDouble(&ok);
        if (!ok || !std::isfinite(n))
            return fallback;
        return int(std::clamp(std::round(n), p.value(QStringLiteral("minimum")).toDouble(),
            p.value(QStringLiteral("maximum")).toDouble()));
    }
    }
}

namespace
{
QMap<uint32_t, VisualDepthRegistry::ItemFactory>& itemFactories()
{
    static QMap<uint32_t, VisualDepthRegistry::ItemFactory> factories;
    return factories;
}
}
void VisualDepthRegistry::registerItemFactory(uint32_t visual, ItemFactory factory)
{
    Q_ASSERT(!itemFactories().contains(visual));
    itemFactories().insert(visual, std::move(factory));
}
LivingVisualItem* VisualDepthRegistry::createItem(uint32_t visual, QQuickItem* parent)
{
    // Metadata construction is complete before owned registrations run. An
    // adapter may safely resolve its stable visual ID while registering.
    static const bool initialized=[] {initializeDepthAdapters();return true;}();
    Q_UNUSED(initialized);
    const auto factory = itemFactories().value(visual);
    return factory ? factory(parent) : new LivingVisualItem(parent);
}

#ifdef PVS_ENABLE_VISUAL_CAPTURE
void VisualDepthRegistry::setAvailabilityForTest(const QString &visual,std::optional<bool> enabled) {
    if(enabled)availabilityOverrides().insert(visual,*enabled);else availabilityOverrides().remove(visual);
}
#endif

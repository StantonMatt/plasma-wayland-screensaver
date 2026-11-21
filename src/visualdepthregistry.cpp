// SPDX-License-Identifier: GPL-3.0-or-later
#include "visualdepthregistry.h"
#include "livingvisualitem.h"
#include <QMap>
#include <QStringList>
#include <cmath>
void initializeDepthResources();
void initializeDepthAdapters();
namespace
{
struct Registry
{
    QMap<QString, uint32_t> ids;
    QMap<QString, QVariantList> schemas;
    Registry()
    {
        initializeDepthResources();
        for (size_t i = 0;; ++i) {
            const uint8_t* key = nullptr;
            size_t length = 0;
            const auto id = visuals_depth_registry_entry(i, &key, &length);
            if (!id)
                break;
            const auto name = QString::fromUtf8(reinterpret_cast<const char*>(key), qsizetype(length));
            ids.insert(name, id);
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
            schemas.insert(name, entries);
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
QStringList VisualDepthRegistry::keys()
{
    return registry().ids.keys();
}
uint32_t VisualDepthRegistry::id(const QString& visual)
{
    return registry().ids.value(visual);
}
bool VisualDepthRegistry::available(const QString& visual)
{
#ifdef PVS_ENABLE_VISUAL_CAPTURE
    if (availabilityOverrides().contains(visual))
        return availabilityOverrides().value(visual);
#endif
    const uint32_t visualId = id(visual);
    if (visuals_depth_available(visualId))
        return true;
#ifdef PVS_ENABLE_VISUAL_CAPTURE
    // A private benchmark can exercise an implemented slot before release.
    return visualId && !qEnvironmentVariableIsEmpty("PVS_PRIVATE_HARNESS_ROOT")
        && qEnvironmentVariable("PVS_CAPTURE_DEPTH_CANDIDATE") == visual;
#else
    return false;
#endif
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

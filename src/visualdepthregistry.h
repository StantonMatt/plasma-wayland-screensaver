// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include "visuals_depth.h"
#include <QStringList>
#include <functional>
#ifdef PVS_ENABLE_VISUAL_CAPTURE
#include <optional>
#endif
class LivingVisualItem;
class QQuickItem;
#include <QVariantList>
#include <QVariantMap>

namespace VisualDepthRegistry
{
using ItemFactory = std::function<LivingVisualItem*(QQuickItem*)>;
void registerItemFactory(uint32_t visual, ItemFactory);
LivingVisualItem* createItem(uint32_t visual, QQuickItem* parent);
QStringList keys();
uint32_t id(const QString& visual);
bool available(const QString& visual);
#ifdef PVS_ENABLE_VISUAL_CAPTURE
void setAvailabilityForTest(const QString &visual,std::optional<bool>);
#endif
QVariantList schema(const QString& visual);
QVariantMap defaults(const QString& visual);
QVariant normalize(const QVariantMap& entry, const QVariant& value);
}

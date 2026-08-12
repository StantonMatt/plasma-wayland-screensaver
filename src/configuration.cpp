// SPDX-License-Identifier: GPL-3.0-or-later
#include "configuration.h"

#include <KConfig>
#include <KConfigGroup>

#include <algorithm>

Configuration::Configuration(const QString &filePath, QObject *parent)
    : QObject(parent)
    , m_config(filePath.isEmpty()
                   ? std::make_unique<KConfig>(QStringLiteral("plasma-visual-screensaverrc"))
                   : std::make_unique<KConfig>(filePath, KConfig::SimpleConfig))
{
    reload();
}

Configuration::~Configuration() = default;

int Configuration::idleMinutes() const { return m_idleMinutes; }
QString Configuration::visualModule() const { return m_visualModule; }
bool Configuration::showClock() const { return m_showClock; }
int Configuration::frameRate() const { return m_frameRate; }
bool Configuration::reducedMotion() const { return m_reducedMotion; }
QString Configuration::monitorBehavior() const { return m_monitorBehavior; }

template<typename T>
void Configuration::update(T &member, const T &value)
{
    if (member == value) {
        return;
    }
    member = value;
    Q_EMIT changed();
}

void Configuration::setIdleMinutes(int value) { update(m_idleMinutes, std::clamp(value, 1, 240)); }
void Configuration::setVisualModule(const QString &value)
{
    update(m_visualModule, value == QStringLiteral("orbs") ? value : QStringLiteral("aurora"));
}
void Configuration::setShowClock(bool value) { update(m_showClock, value); }
void Configuration::setFrameRate(int value)
{
    const int normalized = value <= 15 ? 15 : (value <= 30 ? 30 : 60);
    update(m_frameRate, normalized);
}
void Configuration::setReducedMotion(bool value) { update(m_reducedMotion, value); }
void Configuration::setMonitorBehavior(const QString &value)
{
    update(m_monitorBehavior, value == QStringLiteral("synchronized") ? value : QStringLiteral("independent"));
}

void Configuration::assignDefaults()
{
    setIdleMinutes(10);
    setVisualModule(QStringLiteral("aurora"));
    setShowClock(true);
    setFrameRate(30);
    setReducedMotion(false);
    setMonitorBehavior(QStringLiteral("independent"));
}

void Configuration::reload()
{
    m_config->reparseConfiguration();
    const KConfigGroup general(m_config.get(), QStringLiteral("General"));
    setIdleMinutes(general.readEntry("IdleMinutes", 10));
    setVisualModule(general.readEntry("VisualModule", QStringLiteral("aurora")));
    setShowClock(general.readEntry("ShowClock", true));
    setFrameRate(general.readEntry("FrameRate", 30));
    setReducedMotion(general.readEntry("ReducedMotion", false));
    setMonitorBehavior(general.readEntry("MonitorBehavior", QStringLiteral("independent")));
}

void Configuration::save()
{
    KConfigGroup general(m_config.get(), QStringLiteral("General"));
    general.writeEntry("IdleMinutes", m_idleMinutes);
    general.writeEntry("VisualModule", m_visualModule);
    general.writeEntry("ShowClock", m_showClock);
    general.writeEntry("FrameRate", m_frameRate);
    general.writeEntry("ReducedMotion", m_reducedMotion);
    general.writeEntry("MonitorBehavior", m_monitorBehavior);
    m_config->sync();
    Q_EMIT saved();
}

void Configuration::restoreDefaults()
{
    assignDefaults();
}

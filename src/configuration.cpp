// SPDX-License-Identifier: GPL-3.0-or-later
#include "configuration.h"

#include <KConfig>
#include <KConfigGroup>
#include <QStringList>

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
QString Configuration::backgroundStyle() const { return m_backgroundStyle; }
bool Configuration::showClock() const { return m_showClock; }
QString Configuration::clockMovement() const { return m_clockMovement; }
QString Configuration::clockSpeed() const { return m_clockSpeed; }
int Configuration::frameRate() const { return m_frameRate; }
bool Configuration::reducedMotion() const { return m_reducedMotion; }
QString Configuration::monitorBehavior() const { return m_monitorBehavior; }
bool Configuration::coverPanels() const { return m_coverPanels; }

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
    static const QStringList modules = {
        QStringLiteral("aurora"),
        QStringLiteral("orbs"),
        QStringLiteral("none"),
        QStringLiteral("bounce"),
    };
    update(m_visualModule, modules.contains(value) ? value : QStringLiteral("aurora"));
}
void Configuration::setBackgroundStyle(const QString &value)
{
    static const QStringList backgrounds = {
        QStringLiteral("black"),
        QStringLiteral("midnight"),
        QStringLiteral("ocean"),
        QStringLiteral("plum"),
    };
    update(m_backgroundStyle, backgrounds.contains(value) ? value : QStringLiteral("midnight"));
}
void Configuration::setShowClock(bool value) { update(m_showClock, value); }
void Configuration::setClockMovement(const QString &value)
{
    update(m_clockMovement, value == QStringLiteral("center") ? value : QStringLiteral("bounce"));
}
void Configuration::setClockSpeed(const QString &value)
{
    static const QStringList speeds = {
        QStringLiteral("slow"),
        QStringLiteral("normal"),
        QStringLiteral("fast"),
    };
    update(m_clockSpeed, speeds.contains(value) ? value : QStringLiteral("normal"));
}
void Configuration::setFrameRate(int value)
{
    const int normalized = value <= 15 ? 15 : (value <= 30 ? 30 : 60);
    update(m_frameRate, normalized);
}
void Configuration::setReducedMotion(bool value) { update(m_reducedMotion, value); }
void Configuration::setMonitorBehavior(const QString &value)
{
    static const QStringList behaviors = {
        QStringLiteral("independent"),
        QStringLiteral("synchronized"),
        QStringLiteral("seamless"),
    };
    update(m_monitorBehavior, behaviors.contains(value) ? value : QStringLiteral("independent"));
}
void Configuration::setCoverPanels(bool value) { update(m_coverPanels, value); }

void Configuration::assignDefaults()
{
    setIdleMinutes(10);
    setVisualModule(QStringLiteral("aurora"));
    setBackgroundStyle(QStringLiteral("midnight"));
    setShowClock(true);
    setClockMovement(QStringLiteral("bounce"));
    setClockSpeed(QStringLiteral("normal"));
    setFrameRate(30);
    setReducedMotion(false);
    setMonitorBehavior(QStringLiteral("independent"));
    setCoverPanels(true);
}

void Configuration::reload()
{
    m_config->reparseConfiguration();
    const KConfigGroup general(m_config.get(), QStringLiteral("General"));
    setIdleMinutes(general.readEntry("IdleMinutes", 10));
    QString visual = general.readEntry("VisualModule", QStringLiteral("aurora"));
    const QString legacyBackground = visual == QStringLiteral("black") || visual == QStringLiteral("bounce")
        ? QStringLiteral("black") : QStringLiteral("midnight");
    if (visual == QStringLiteral("black")) {
        visual = QStringLiteral("none");
    }
    setVisualModule(visual);
    setBackgroundStyle(general.readEntry("BackgroundStyle", legacyBackground));
    setShowClock(general.readEntry("ShowClock", true));
    setClockMovement(general.readEntry("ClockMovement", QStringLiteral("bounce")));
    setClockSpeed(general.readEntry("ClockSpeed", QStringLiteral("normal")));
    setFrameRate(general.readEntry("FrameRate", 30));
    setReducedMotion(general.readEntry("ReducedMotion", false));
    setMonitorBehavior(general.readEntry("MonitorBehavior", QStringLiteral("independent")));
    setCoverPanels(general.readEntry("CoverPanels", true));
}

void Configuration::save()
{
    KConfigGroup general(m_config.get(), QStringLiteral("General"));
    general.writeEntry("IdleMinutes", m_idleMinutes);
    general.writeEntry("VisualModule", m_visualModule);
    general.writeEntry("BackgroundStyle", m_backgroundStyle);
    general.writeEntry("ShowClock", m_showClock);
    general.writeEntry("ClockMovement", m_clockMovement);
    general.writeEntry("ClockSpeed", m_clockSpeed);
    general.writeEntry("FrameRate", m_frameRate);
    general.writeEntry("ReducedMotion", m_reducedMotion);
    general.writeEntry("MonitorBehavior", m_monitorBehavior);
    general.writeEntry("CoverPanels", m_coverPanels);
    m_config->sync();
    Q_EMIT saved();
}

void Configuration::restoreDefaults()
{
    assignDefaults();
}

// SPDX-License-Identifier: GPL-3.0-or-later
#include "configuration.h"

#include <KConfig>
#include <KConfigGroup>
#include <QStringList>

#include <algorithm>
#include <cstdlib>

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
int Configuration::animationSpeed() const { return m_animationSpeed; }
int Configuration::animationDensity() const { return m_animationDensity; }
int Configuration::animationScale() const { return m_animationScale; }
QString Configuration::animationPalette() const { return m_animationPalette; }
int Configuration::trailAmount() const { return m_trailAmount; }
int Configuration::ballCount() const { return m_ballCount; }
int Configuration::ballGravity() const { return m_ballGravity; }
int Configuration::ballElasticity() const { return m_ballElasticity; }
bool Configuration::ballCollisions() const { return m_ballCollisions; }
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
        QStringLiteral("starfield"),
        QStringLiteral("matrix"),
        QStringLiteral("kaleidoscope"),
        QStringLiteral("fireflies"),
        QStringLiteral("ribbons"),
        QStringLiteral("constellation"),
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
void Configuration::setAnimationSpeed(int value) { update(m_animationSpeed, std::clamp(value, 10, 300)); }
void Configuration::setAnimationDensity(int value) { update(m_animationDensity, std::clamp(value, 10, 100)); }
void Configuration::setAnimationScale(int value) { update(m_animationScale, std::clamp(value, 25, 200)); }
void Configuration::setAnimationPalette(const QString &value)
{
    static const QStringList palettes = {
        QStringLiteral("ocean"),
        QStringLiteral("spectrum"),
        QStringLiteral("ember"),
        QStringLiteral("forest"),
        QStringLiteral("mono"),
        QStringLiteral("pastel"),
    };
    update(m_animationPalette, palettes.contains(value) ? value : QStringLiteral("ocean"));
}
void Configuration::setTrailAmount(int value) { update(m_trailAmount, std::clamp(value, 0, 100)); }
void Configuration::setBallCount(int value) { update(m_ballCount, std::clamp(value, 1, 20)); }
void Configuration::setBallGravity(int value) { update(m_ballGravity, std::clamp(value, -100, 100)); }
void Configuration::setBallElasticity(int value) { update(m_ballElasticity, std::clamp(value, 50, 100)); }
void Configuration::setBallCollisions(bool value) { update(m_ballCollisions, value); }
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
    if (value == 0) {
        update(m_frameRate, 0);
        return;
    }
    if (value < 0) {
        update(m_frameRate, 15);
        return;
    }
    static const QList<int> fixedRates = {
        15, 24, 30, 45, 60, 75, 90, 100, 120, 144, 165, 175, 200, 240,
    };
    if (fixedRates.contains(value)) {
        update(m_frameRate, value);
        return;
    }
    const auto closest = std::min_element(fixedRates.cbegin(), fixedRates.cend(),
                                          [value](int left, int right) {
                                              return std::abs(left - value) < std::abs(right - value);
                                          });
    update(m_frameRate, closest == fixedRates.cend() ? 30 : *closest);
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
    setAnimationSpeed(100);
    setAnimationDensity(50);
    setAnimationScale(100);
    setAnimationPalette(QStringLiteral("ocean"));
    setTrailAmount(35);
    setBallCount(5);
    setBallGravity(35);
    setBallElasticity(92);
    setBallCollisions(true);
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
    setAnimationSpeed(general.readEntry("AnimationSpeed", 100));
    setAnimationDensity(general.readEntry("AnimationDensity", 50));
    setAnimationScale(general.readEntry("AnimationScale", 100));
    setAnimationPalette(general.readEntry("AnimationPalette", QStringLiteral("ocean")));
    setTrailAmount(general.readEntry("TrailAmount", 35));
    setBallCount(general.readEntry("BallCount", 5));
    setBallGravity(general.readEntry("BallGravity", 35));
    setBallElasticity(general.readEntry("BallElasticity", 92));
    setBallCollisions(general.readEntry("BallCollisions", true));
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
    general.writeEntry("AnimationSpeed", m_animationSpeed);
    general.writeEntry("AnimationDensity", m_animationDensity);
    general.writeEntry("AnimationScale", m_animationScale);
    general.writeEntry("AnimationPalette", m_animationPalette);
    general.writeEntry("TrailAmount", m_trailAmount);
    general.writeEntry("BallCount", m_ballCount);
    general.writeEntry("BallGravity", m_ballGravity);
    general.writeEntry("BallElasticity", m_ballElasticity);
    general.writeEntry("BallCollisions", m_ballCollisions);
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

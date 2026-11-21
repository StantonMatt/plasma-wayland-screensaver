// SPDX-License-Identifier: GPL-3.0-or-later
#include "configuration.h"
#include "visualdepthregistry.h"
#include <QDebug>
#include <cmath>

#include <KConfig>
#include <KConfigGroup>
#include <QStringList>

#include <algorithm>
#include <cstdlib>

namespace {
const QStringList &visualModules()
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
        QStringLiteral("snakes"),
    };
    return modules;
}
const QStringList &animationKeys()
{
    static const QStringList keys = {QStringLiteral("animationSpeed"), QStringLiteral("animationDensity"),
        QStringLiteral("animationScale"), QStringLiteral("animationPalette"), QStringLiteral("trailAmount")};
    return keys;
}
const QStringList &generalKeys()
{
    static const QStringList keys = {QStringLiteral("idleMinutes"), QStringLiteral("monitorBehavior"),
        QStringLiteral("coverPanels"), QStringLiteral("frameRate"), QStringLiteral("reducedMotion")};
    return keys;
}
QString diskKey(QString key)
{
    key[0] = key[0].toUpper();
    return key;
}
bool oldDefaultTuple(const QVariantMap &profile) {
    return profile.value(QStringLiteral("animationSpeed")).toInt()==100
        && profile.value(QStringLiteral("animationDensity")).toInt()==50
        && profile.value(QStringLiteral("animationScale")).toInt()==100
        && profile.value(QStringLiteral("animationPalette")).toString()==QStringLiteral("ocean")
        && profile.value(QStringLiteral("trailAmount")).toInt()==35;
}
QVariantMap pageValues(QVariantMap values, const QString &page)
{
    if (page.isEmpty()) return values;
    if (page != QStringLiteral("appearance") && page != QStringLiteral("general")) return {};
    for (auto it = values.begin(); it != values.end();) {
        if (generalKeys().contains(it.key()) != (page == QStringLiteral("general"))) it = values.erase(it);
        else ++it;
    }
    return values;
}
} // namespace

Configuration::Configuration(const QString &filePath, QObject *parent)
    : QObject(parent)
    , m_config(filePath.isEmpty()
                   ? std::make_unique<KConfig>(QStringLiteral("plasma-visual-screensaverrc"))
                   : std::make_unique<KConfig>(filePath, KConfig::SimpleConfig))
{
    reload();
}

Configuration::~Configuration() = default;

QVariantMap Configuration::visualParameters() const
{
    auto values = m_visualParameters.value(m_visualModule);
    if (m_visualModule == QStringLiteral("bounce"))
    {
        values.insert(QStringLiteral("count"), m_ballCount);
        values.insert(QStringLiteral("gravity"), m_ballGravity);
        values.insert(QStringLiteral("elasticity"), m_ballElasticity);
        values.insert(QStringLiteral("collisions"), m_ballCollisions);
    }
    return values;
}
bool Configuration::depthAvailable() const
{
    return VisualDepthRegistry::available(m_visualModule);
}
QVariant Configuration::visualParameter(const QString& key) const
{
    return visualParameters().value(key);
}
QVariantList Configuration::visualSchema(const QString& visual) const
{
    return VisualDepthRegistry::schema(visual.isEmpty() ? m_visualModule : visual);
}
void Configuration::setVisualParameter(const QString& key, const QVariant& value)
{
    for (const auto& v : visualSchema())
    {
        const auto entry = v.toMap();
        if (entry.value(QStringLiteral("key")).toString() != key)
            continue;
        const auto normalized = VisualDepthRegistry::normalize(entry, value);
        if (m_visualModule == QStringLiteral("bounce"))
        {
            if (key == QStringLiteral("count"))
            {
                setBallCount(normalized.toInt());
                return;
            }
            if (key == QStringLiteral("gravity"))
            {
                setBallGravity(normalized.toInt());
                return;
            }
            if (key == QStringLiteral("elasticity"))
            {
                setBallElasticity(normalized.toInt());
                return;
            }
            if (key == QStringLiteral("collisions"))
            {
                setBallCollisions(normalized.toBool());
                return;
            }
        }
        if (m_visualParameters[m_visualModule].value(key) == normalized)
            return;
        m_visualParameters[m_visualModule].insert(key, normalized);
        m_depthDirtyKeys[m_visualModule].insert(key);
        if (m_updateDepth > 0)
            m_changedPending = true;
        else
            Q_EMIT changed();
        return;
    }
}

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
int Configuration::snakeAggression() const { return m_snakeAggression; }
int Configuration::snakeIntelligence() const { return m_snakeIntelligence; }
bool Configuration::snakeSelfCollisions() const { return m_snakeSelfCollisions; }
bool Configuration::snakeLengthLimit() const { return m_snakeLengthLimit; }
bool Configuration::snakePowerUps() const { return m_snakePowerUps; }
bool Configuration::snakeStorePowerUps() const { return m_snakeStorePowerUps; }
bool Configuration::snakeWorldEvents() const { return m_snakeWorldEvents; }
bool Configuration::snakeDeadlyWalls() const { return m_snakeDeadlyWalls; }
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
    if (m_updateDepth > 0) {
        m_changedPending = true;
    } else {
        Q_EMIT changed();
    }
}

void Configuration::beginUpdate()
{
    ++m_updateDepth;
}

void Configuration::endUpdate()
{
    Q_ASSERT(m_updateDepth > 0);
    --m_updateDepth;
    if (m_updateDepth == 0 && m_changedPending) {
        m_changedPending = false;
        Q_EMIT changed();
    }
}

void Configuration::setIdleMinutes(int value) { update(m_idleMinutes, std::clamp(value, 1, 240)); }
void Configuration::setVisualModule(const QString &value)
{
    const QString module = visualModules().contains(value) ? value : QStringLiteral("aurora");
    if (module == m_visualModule) return;
    beginUpdate();
    m_animationSettings.insert(m_visualModule, activeAnimationSettings());
    update(m_visualModule, module);
    applyAnimationSettings(m_animationSettings.value(module));
    endUpdate();
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
void Configuration::setBallCount(int value) { update(m_ballCount, std::clamp(value, 1, 60)); }
void Configuration::setBallGravity(int value) { update(m_ballGravity, std::clamp(value, -100, 100)); }
void Configuration::setBallElasticity(int value) { update(m_ballElasticity, std::clamp(value, 50, 100)); }
void Configuration::setBallCollisions(bool value) { update(m_ballCollisions, value); }
void Configuration::setSnakeAggression(int value) { update(m_snakeAggression, std::clamp(value, 0, 100)); }
void Configuration::setSnakeIntelligence(int value)
{
    update(m_snakeIntelligence, std::clamp(value, 0, 100));
}
void Configuration::setSnakeSelfCollisions(bool value) { update(m_snakeSelfCollisions, value); }
void Configuration::setSnakeLengthLimit(bool value) { update(m_snakeLengthLimit, value); }
void Configuration::setSnakePowerUps(bool value) { update(m_snakePowerUps, value); }
void Configuration::setSnakeStorePowerUps(bool value) { update(m_snakeStorePowerUps, value); }
void Configuration::setSnakeWorldEvents(bool value) { update(m_snakeWorldEvents, value); }
void Configuration::setSnakeDeadlyWalls(bool value) { update(m_snakeDeadlyWalls, value); }
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

void Configuration::apply(const QVariantMap &settings)
{
    beginUpdate();
    if (settings.contains(QStringLiteral("idleMinutes"))) {
        setIdleMinutes(settings.value(QStringLiteral("idleMinutes")).toInt());
    }
    if (settings.contains(QStringLiteral("visualModule"))) {
        setVisualModule(settings.value(QStringLiteral("visualModule")).toString());
    }
    if (settings.contains(QStringLiteral("animationSettings"))) {
        const QVariantMap profiles = settings.value(QStringLiteral("animationSettings")).toMap();
        const QString selected = m_visualModule;
        for (const QString &module : visualModules()) {
            if (!profiles.contains(module)) continue;
            setVisualModule(module);
            applyAnimationSettings(profiles.value(module).toMap());
            m_animationSettings.insert(module, activeAnimationSettings());
        }
        setVisualModule(selected);
    }
    if (settings.contains(QStringLiteral("visualParameterProfiles")))
    {
        const auto profiles = settings.value(QStringLiteral("visualParameterProfiles")).toMap();
        const auto selected = m_visualModule;
        for (auto it = profiles.cbegin(); it != profiles.cend(); ++it)
        {
            if (!visualModules().contains(it.key()))
                continue;
            setVisualModule(it.key());
            const auto parameters = it.value().toMap();
            for (auto p = parameters.cbegin(); p != parameters.cend(); ++p)
                setVisualParameter(p.key(), p.value());
        }
        setVisualModule(selected);
    }
    if (settings.contains(QStringLiteral("backgroundStyle"))) {
        setBackgroundStyle(settings.value(QStringLiteral("backgroundStyle")).toString());
    }
    if (settings.contains(QStringLiteral("animationSpeed"))) {
        setAnimationSpeed(settings.value(QStringLiteral("animationSpeed")).toInt());
    }
    if (settings.contains(QStringLiteral("animationDensity"))) {
        setAnimationDensity(settings.value(QStringLiteral("animationDensity")).toInt());
    }
    if (settings.contains(QStringLiteral("animationScale"))) {
        setAnimationScale(settings.value(QStringLiteral("animationScale")).toInt());
    }
    if (settings.contains(QStringLiteral("animationPalette"))) {
        setAnimationPalette(settings.value(QStringLiteral("animationPalette")).toString());
    }
    if (settings.contains(QStringLiteral("trailAmount"))) {
        setTrailAmount(settings.value(QStringLiteral("trailAmount")).toInt());
    }
    if (settings.contains(QStringLiteral("ballCount"))) {
        setBallCount(settings.value(QStringLiteral("ballCount")).toInt());
    }
    if (settings.contains(QStringLiteral("ballGravity"))) {
        setBallGravity(settings.value(QStringLiteral("ballGravity")).toInt());
    }
    if (settings.contains(QStringLiteral("ballElasticity"))) {
        setBallElasticity(settings.value(QStringLiteral("ballElasticity")).toInt());
    }
    if (settings.contains(QStringLiteral("ballCollisions"))) {
        setBallCollisions(settings.value(QStringLiteral("ballCollisions")).toBool());
    }
    if (settings.contains(QStringLiteral("snakeAggression"))) {
        setSnakeAggression(settings.value(QStringLiteral("snakeAggression")).toInt());
    }
    if (settings.contains(QStringLiteral("snakeIntelligence"))) {
        setSnakeIntelligence(settings.value(QStringLiteral("snakeIntelligence")).toInt());
    }
    if (settings.contains(QStringLiteral("snakeSelfCollisions"))) {
        setSnakeSelfCollisions(settings.value(QStringLiteral("snakeSelfCollisions")).toBool());
    }
    if (settings.contains(QStringLiteral("snakeWorldEvents"))) {
        setSnakeWorldEvents(settings.value(QStringLiteral("snakeWorldEvents")).toBool());
    }
    if (settings.contains(QStringLiteral("snakeLengthLimit"))) {
        setSnakeLengthLimit(settings.value(QStringLiteral("snakeLengthLimit")).toBool());
    }
    if (settings.contains(QStringLiteral("snakeStorePowerUps"))) {
        setSnakeStorePowerUps(settings.value(QStringLiteral("snakeStorePowerUps")).toBool());
    }
    if (settings.contains(QStringLiteral("snakePowerUps"))) {
        setSnakePowerUps(settings.value(QStringLiteral("snakePowerUps")).toBool());
    }
    if (settings.contains(QStringLiteral("snakeDeadlyWalls"))) {
        setSnakeDeadlyWalls(settings.value(QStringLiteral("snakeDeadlyWalls")).toBool());
    }
    if (settings.contains(QStringLiteral("showClock"))) {
        setShowClock(settings.value(QStringLiteral("showClock")).toBool());
    }
    if (settings.contains(QStringLiteral("clockMovement"))) {
        setClockMovement(settings.value(QStringLiteral("clockMovement")).toString());
    }
    if (settings.contains(QStringLiteral("clockSpeed"))) {
        setClockSpeed(settings.value(QStringLiteral("clockSpeed")).toString());
    }
    if (settings.contains(QStringLiteral("frameRate"))) {
        setFrameRate(settings.value(QStringLiteral("frameRate")).toInt());
    }
    if (settings.contains(QStringLiteral("reducedMotion"))) {
        setReducedMotion(settings.value(QStringLiteral("reducedMotion")).toBool());
    }
    if (settings.contains(QStringLiteral("monitorBehavior"))) {
        setMonitorBehavior(settings.value(QStringLiteral("monitorBehavior")).toString());
    }
    if (settings.contains(QStringLiteral("coverPanels"))) {
        setCoverPanels(settings.value(QStringLiteral("coverPanels")).toBool());
    }
    if (settings.contains(QStringLiteral("clockMode"))) setClockMode(settings.value(QStringLiteral("clockMode")).toInt());
    // Undo restores preservation state after setters have applied normalized values.
    const auto preservation = settings.value(QStringLiteral("visualParameterPreservation")).toMap();
    for (auto it = preservation.cbegin(); it != preservation.cend(); ++it) {
        if (!visualModules().contains(it.key())) continue;
        const auto state = it.value().toMap();
        const auto raw = state.value(QStringLiteral("rawValues")).toMap();
        const auto keys = state.value(QStringLiteral("dirtyKeys")).toStringList();
        const QSet<QString> dirty(keys.cbegin(), keys.cend());
        if (m_depthRawValues.value(it.key()) != raw || m_depthDirtyKeys.value(it.key()) != dirty)
            m_changedPending = true;
        m_depthRawValues.insert(it.key(), raw);
        m_depthDirtyKeys.insert(it.key(), dirty);
    }
    endUpdate();
}

QVariantMap Configuration::defaults(const QString &page) const
{
    QVariantMap values = {
        {QStringLiteral("idleMinutes"), 10},
        {QStringLiteral("visualModule"), QStringLiteral("aurora")},
        {QStringLiteral("backgroundStyle"), QStringLiteral("midnight")},
        {QStringLiteral("animationSpeed"), 100},
        {QStringLiteral("animationDensity"), 50},
        {QStringLiteral("animationScale"), 100},
        {QStringLiteral("animationPalette"), QStringLiteral("ocean")},
        {QStringLiteral("trailAmount"), 35},
        {QStringLiteral("ballCount"), 5},
        {QStringLiteral("ballGravity"), 35},
        {QStringLiteral("ballElasticity"), 92},
        {QStringLiteral("ballCollisions"), true},
        {QStringLiteral("snakeIntelligence"), 75},
        {QStringLiteral("snakeAggression"), 100},
        {QStringLiteral("snakeSelfCollisions"), false},
        {QStringLiteral("snakeDeadlyWalls"), true},
        {QStringLiteral("snakeLengthLimit"), false},
        {QStringLiteral("snakePowerUps"), true},
        {QStringLiteral("snakeStorePowerUps"), true},
        {QStringLiteral("snakeWorldEvents"), true},
        {QStringLiteral("showClock"), true},
        {QStringLiteral("clockMovement"), QStringLiteral("bounce")},
        {QStringLiteral("clockSpeed"), QStringLiteral("normal")},
        {QStringLiteral("frameRate"), 30},
        {QStringLiteral("reducedMotion"), false},
        {QStringLiteral("monitorBehavior"), QStringLiteral("independent")},
        {QStringLiteral("coverPanels"), true}
    };
    QVariantMap animation;
    for (const QString &key : animationKeys()) animation.insert(key, values.value(key));
    QVariantMap profiles;
    for (const QString &module : visualModules()) {
        auto profile=animation;
        if(VisualDepthRegistry::available(module)&&module==QStringLiteral("kaleidoscope"))profile.insert(QStringLiteral("animationPalette"),QStringLiteral("spectrum"));
        profiles.insert(module,profile);
    }
    if(VisualDepthRegistry::available(QStringLiteral("bounce"))) {
        values.insert(QStringLiteral("ballCount"),12);values.insert(QStringLiteral("ballElasticity"),90);
    }
    values.insert(QStringLiteral("animationSettings"), profiles);
    QVariantMap parameterProfiles;
    for (const auto& module : visualModules())
    {
        auto own = VisualDepthRegistry::defaults(module);
        if (module == QStringLiteral("bounce"))
            for (const auto& key : { QStringLiteral("count"), QStringLiteral("gravity"), QStringLiteral("elasticity"),
                     QStringLiteral("collisions") })
                own.remove(key);
        parameterProfiles.insert(module, own);
    }
    values.insert(QStringLiteral("visualParameterProfiles"), parameterProfiles);
    if (page == QStringLiteral("appearance"))
    {
        // Keep gallery selection, other profiles and their rules. Undo still uses
        // the complete snapshot; Defaults edits only the selected profile.
        const auto current = snapshot();
        for (auto it = current.cbegin(); it != current.cend(); ++it)
        {
            const bool selectedBall
                = m_visualModule == QStringLiteral("bounce") && it.key().startsWith(QStringLiteral("ball"));
            const bool selectedSnake
                = m_visualModule == QStringLiteral("snakes") && it.key().startsWith(QStringLiteral("snake"));
            if ((it.key().startsWith(QStringLiteral("ball")) && !selectedBall)
                || (it.key().startsWith(QStringLiteral("snake")) && !selectedSnake))
                values.insert(it.key(), it.value());
        }
        auto kept = current.value(QStringLiteral("animationSettings")).toMap();
        const auto selectedDefaults=profiles.value(m_visualModule).toMap();
        kept.insert(m_visualModule, selectedDefaults);
        values.insert(QStringLiteral("animationPalette"),selectedDefaults.value(QStringLiteral("animationPalette")));
        values.insert(QStringLiteral("animationSettings"), kept);
        auto own = current.value(QStringLiteral("visualParameterProfiles")).toMap();
        own.insert(m_visualModule, parameterProfiles.value(m_visualModule));
        values.insert(QStringLiteral("visualParameterProfiles"), own);
        values.insert(QStringLiteral("visualModule"), m_visualModule);
        if (depthAvailable() && m_visualModule == QStringLiteral("kaleidoscope"))
            values.insert(QStringLiteral("animationPalette"), QStringLiteral("spectrum"));
    }
    return pageValues(values, page);
}

QVariantMap Configuration::snapshot(const QString &page, bool includePreservation) const
{
    QVariantMap values;
    const QVariantMap defaultValues = defaults();
    for (auto it = defaultValues.cbegin(); it != defaultValues.cend(); ++it) {
        if (it.key() != QStringLiteral("animationSettings") && it.key() != QStringLiteral("visualParameterProfiles"))
            values.insert(it.key(), property(it.key().toUtf8().constData()));
    }
    QVariantMap profiles;
    for (const QString &module : visualModules()) {
        profiles.insert(module, module == m_visualModule ? activeAnimationSettings() : m_animationSettings.value(module));
    }
    values.insert(QStringLiteral("animationSettings"), profiles);
    QVariantMap own;
    for (const auto& module : visualModules())
        own.insert(module, m_visualParameters.value(module));
    values.insert(QStringLiteral("visualParameterProfiles"), own);
    if (includePreservation) {
        QVariantMap preservation;
        for (const auto& module : visualModules()) {
            const auto dirty = m_depthDirtyKeys.value(module);
            QStringList keys(dirty.cbegin(), dirty.cend());
            keys.sort();
            preservation.insert(module, QVariantMap {
                {QStringLiteral("rawValues"), m_depthRawValues.value(module)},
                {QStringLiteral("dirtyKeys"), keys}});
        }
        values.insert(QStringLiteral("visualParameterPreservation"), preservation);
    }
    return pageValues(values, page);
}

QVariantMap Configuration::activeAnimationSettings() const
{
    return {{QStringLiteral("animationSpeed"), m_animationSpeed},
        {QStringLiteral("animationDensity"), m_animationDensity},
        {QStringLiteral("animationScale"), m_animationScale},
        {QStringLiteral("animationPalette"), m_animationPalette},
        {QStringLiteral("trailAmount"), m_trailAmount}};
}

void Configuration::applyAnimationSettings(const QVariantMap &settings)
{
    for (const QString &key : animationKeys()) {
        if (settings.contains(key)) setProperty(key.toUtf8().constData(), settings.value(key));
    }
}

void Configuration::assignDefaults()
{
    apply(defaults());
}

void Configuration::reload()
{
    m_config->reparseConfiguration();
    const KConfigGroup general(m_config.get(), QStringLiteral("General"));
    // Capture raw key presence before any legacy setters or v0 migration.
    const auto rawGeneralValues=general.entryMap();
    const KConfigGroup rawAnimations(m_config.get(), QStringLiteral("Animations"));
    m_depthRawValues.clear();
    for (const auto& module : visualModules())
    {
        const auto group = rawAnimations.group(module);
        QVariantMap raw;
        for (const auto& key : group.keyList())
            raw.insert(key, group.readEntry(key, QString()));
        m_depthRawValues.insert(module, raw);
    }
    const QVariantMap defaultValues = defaults();
    QVariantMap values;
    for (auto it = defaultValues.cbegin(); it != defaultValues.cend(); ++it) {
        if (it.key() == QStringLiteral("animationSettings") || it.key() == QStringLiteral("visualParameterProfiles"))
            continue;
        values.insert(it.key(), general.readEntry(diskKey(it.key()), it.value()));
    }
    // Read the released Bounce tuple first; upgrade it only as a whole below.
    values.insert(QStringLiteral("ballCount"), general.readEntry("BallCount", 5));
    values.insert(QStringLiteral("ballElasticity"), general.readEntry("BallElasticity", 92));
    const QString legacyVisual = values.value(QStringLiteral("visualModule")).toString();
    if (legacyVisual == QStringLiteral("black")) values[QStringLiteral("visualModule")] = QStringLiteral("none");
    if (!general.hasKey("BackgroundStyle") && (legacyVisual == QStringLiteral("black") || legacyVisual == QStringLiteral("bounce"))) {
        values[QStringLiteral("backgroundStyle")] = QStringLiteral("black");
    }
    beginUpdate();
    assignDefaults();
    apply(values);
    const bool migrate = general.readEntry("AnimationSettingsVersion", 0) < 1;
    const QVariantMap legacyAnimation = activeAnimationSettings(); // normalized by setters
    const KConfigGroup animations(m_config.get(), QStringLiteral("Animations"));
    QVariantMap profiles;
    for (const QString &module : visualModules()) {
        const KConfigGroup group = animations.group(module);
        QVariantMap profile;
        for (const QString &key : animationKeys()) {
            const QVariant fallback = migrate ? legacyAnimation.value(key) : defaultValues.value(key);
            profile.insert(key, group.readEntry(diskKey(key), fallback));
        }
        profiles.insert(module, profile);
    }
    apply({{QStringLiteral("animationSettings"), profiles}});
    // Legacy profiles are the only stored shared values. Keep the pre-activation
    // baseline so an explicit save writes any newly applied defaults.
    m_loadedAnimationSettings = m_animationSettings;
    m_depthVersions.clear();
    m_depthDirtyKeys.clear();
    QVariantMap activatedProfiles;
    for (const auto& module : visualModules())
    {
        const auto group = animations.group(module);
        m_depthVersions.insert(module, group.readEntry("DepthSettingsVersion", 0));
        const auto raw = m_depthRawValues.value(module);
        auto own = VisualDepthRegistry::defaults(module);
        for (const auto& v : VisualDepthRegistry::schema(module))
        {
            const auto p = v.toMap();
            const auto key = p.value(QStringLiteral("key")).toString();
            if (raw.contains(key))
                own.insert(key, VisualDepthRegistry::normalize(p, raw.value(key)));
        }
        // General is the only home of the four released Bounce ball keys.
        if (module == QStringLiteral("bounce"))
            for (const auto& key : { QStringLiteral("count"), QStringLiteral("gravity"), QStringLiteral("elasticity"),
                     QStringLiteral("collisions") })
                own.remove(key);
        m_visualParameters.insert(module, own);
        auto profile = m_animationSettings.value(module);
        auto originalTuple = profile;
        for (const auto& key : animationKeys()) {
            const auto disk = diskKey(key);
            if (raw.contains(disk)) originalTuple.insert(key, raw.value(disk));
            else if (migrate && rawGeneralValues.contains(disk)) originalTuple.insert(key, rawGeneralValues.value(disk));
        }
        if (VisualDepthRegistry::available(module) && module == QStringLiteral("kaleidoscope")) {
            const bool untouched = oldDefaultTuple(originalTuple);
            if (untouched)
                profile.insert(QStringLiteral("animationPalette"), QStringLiteral("spectrum"));
            if (!raw.contains(QStringLiteral("points")))
                m_visualParameters[module].insert(QStringLiteral("points"), untouched ? 8
                    : int(std::round(5. + profile.value(QStringLiteral("animationDensity")).toInt() / 9.)));
        }
        activatedProfiles.insert(module, profile);
    }
    apply({{QStringLiteral("animationSettings"), activatedProfiles}});
    if (VisualDepthRegistry::available(QStringLiteral("bounce"))
        && general.readEntry("BallCount", 5) == 5 && general.readEntry("BallGravity", 35) == 35
        && general.readEntry("BallElasticity", 92) == 92 && general.readEntry("BallCollisions", true)) {
        setBallCount(12);
        setBallElasticity(90);
    }
    m_animationSettings.insert(m_visualModule, activeAnimationSettings());
    m_depthDirtyKeys.clear();
    m_changedPending = true;
    endUpdate();
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
    general.writeEntry("SnakeIntelligence", m_snakeIntelligence);
    general.writeEntry("SnakeAggression", m_snakeAggression);
    general.writeEntry("SnakeSelfCollisions", m_snakeSelfCollisions);
    general.writeEntry("SnakeDeadlyWalls", m_snakeDeadlyWalls);
    general.writeEntry("SnakeLengthLimit", m_snakeLengthLimit);
    general.writeEntry("SnakePowerUps", m_snakePowerUps);
    general.writeEntry("SnakeStorePowerUps", m_snakeStorePowerUps);
    general.writeEntry("SnakeWorldEvents", m_snakeWorldEvents);
    general.writeEntry("ShowClock", m_showClock);
    general.writeEntry("ClockMovement", m_clockMovement);
    general.writeEntry("ClockSpeed", m_clockSpeed);
    general.writeEntry("FrameRate", m_frameRate);
    general.writeEntry("ReducedMotion", m_reducedMotion);
    general.writeEntry("MonitorBehavior", m_monitorBehavior);
    general.writeEntry("CoverPanels", m_coverPanels);
    KConfigGroup animations(m_config.get(), QStringLiteral("Animations"));
    m_animationSettings.insert(m_visualModule, activeAnimationSettings());
    for (const QString &module : visualModules()) {
        KConfigGroup group = animations.group(module);
        const QVariantMap profile = m_animationSettings.value(module);
        for (const QString& key : animationKeys())
        {
            if (!group.hasKey(diskKey(key)) || profile.value(key) != m_loadedAnimationSettings.value(module).value(key))
                group.writeEntry(diskKey(key), profile.value(key));
        }
        if (!VisualDepthRegistry::schema(module).isEmpty())
        {
            const int version = m_depthVersions.value(module);
            if (version <= 1) group.writeEntry("DepthSettingsVersion", 1);
            for (const auto& v : VisualDepthRegistry::schema(module))
            {
                const auto key = v.toMap().value(QStringLiteral("key")).toString();
                if (module == QStringLiteral("bounce")
                    && (key == QStringLiteral("count") || key == QStringLiteral("gravity")
                        || key == QStringLiteral("elasticity") || key == QStringLiteral("collisions")))
                    continue;
                const auto raw = m_depthRawValues.value(module);
                if (m_depthDirtyKeys.value(module).contains(key))
                    group.writeEntry(key, m_visualParameters.value(module).value(key));
                else if (raw.contains(key))
                    // A snapshot may restore a newer raw value after Defaults was saved.
                    group.writeEntry(key, raw.value(key));
                else if (version <= 1 && VisualDepthRegistry::available(module))
                    group.writeEntry(key, m_visualParameters.value(module).value(key));
            }
        }
    }
    general.writeEntry("AnimationSettingsVersion", std::max(1, general.readEntry("AnimationSettingsVersion", 0)));
    if (!m_config->sync())
    {
        qWarning() << "Could not save screensaver configuration";
        return;
    }
    // A later edit may return to a value from the original load. Compare it
    // against the last successful save, not that stale original baseline.
    m_loadedAnimationSettings=m_animationSettings;
    m_depthDirtyKeys.clear();
    for(const auto &module:visualModules()) {
        const auto group=animations.group(module);QVariantMap raw;
        for(const auto &key:group.keyList())raw.insert(key,group.readEntry(key,QString()));
        m_depthRawValues.insert(module,raw);
        m_depthVersions.insert(module,group.readEntry("DepthSettingsVersion",0));
    }
    Q_EMIT saved();
}

void Configuration::restoreDefaults(const QString &page)
{
    if (page.isEmpty()) assignDefaults();
    else apply(defaults(page));
}

int Configuration::clockMode() const
{
    if (!m_showClock) return 0;
    if (m_clockMovement == QStringLiteral("center")) return 1;
    if (m_clockSpeed == QStringLiteral("slow")) return 2;
    return m_clockSpeed == QStringLiteral("fast") ? 4 : 3;
}

void Configuration::setClockMode(int mode)
{
    beginUpdate();
    mode = std::clamp(mode, 0, 4);
    setShowClock(mode != 0);
    if (mode == 1) setClockMovement(QStringLiteral("center"));
    if (mode >= 2) {
        setClockMovement(QStringLiteral("bounce"));
        setClockSpeed(mode == 2 ? QStringLiteral("slow") : mode == 4 ? QStringLiteral("fast") : QStringLiteral("normal"));
    }
    endUpdate();
}

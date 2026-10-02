// SPDX-License-Identifier: GPL-3.0-or-later
#include "snakesimulation.h"
#include "configuration.h"
#include "snakerenderer.h"
#include <QtQml>
#include <algorithm>
#include <bit>
#include <cmath>

namespace {
double extent(double value) { return std::clamp(value, 80.0, 16384.0); }
template<typename T> void grow(std::vector<T> &buffer, size_t count)
{
    if (count > buffer.capacity()) buffer.reserve(std::max(count, buffer.capacity() * 2));
    buffer.resize(count);
}
}

SnakeSimulation::SnakeSimulation(const snakes_core_config &config, QObject *parent)
    : QObject(parent), m_config(config), m_viewSize(config.width, config.height)
{
    snakes_core_world *world = nullptr;
    if (snakes_core_create(&config, &world) != SNAKES_CORE_OK) {
        qWarning("Could not create native snake world");
        return;
    }
    m_world.reset(world);
    exportFrame();
}
SnakeSimulation::~SnakeSimulation() = default;

snakes_core_config SnakeSimulation::configuration(const Configuration &settings,
                                                    double width, double height, quint32 seed)
{
    return {extent(width), extent(height), double(settings.animationDensity()),
            double(settings.trailAmount()), double(settings.animationScale()),
            double(settings.animationSpeed()), double(settings.snakeIntelligence()),
            std::bit_cast<qint32>(seed), 6, uint32_t(settings.snakeSelfCollisions()),
            uint32_t(settings.snakeDeadlyWalls())};
}

QVector<QColor> SnakeSimulation::colors(const QString &palette)
{
    if (palette == QStringLiteral("spectrum"))
        return {QColor("#ff477e"), QColor("#ffbe0b"), QColor("#42e2b8"), QColor("#3a86ff"), QColor("#b967ff"), QColor("#fb5607")};
    if (palette == QStringLiteral("ember"))
        return {QColor("#fff1a8"), QColor("#ffc857"), QColor("#ff7b42"), QColor("#ef3e36"), QColor("#9c1c28"), QColor("#ffd6a5")};
    if (palette == QStringLiteral("forest"))
        return {QColor("#d8f3dc"), QColor("#95d5b2"), QColor("#52b788"), QColor("#2d6a4f"), QColor("#b7e4c7"), QColor("#74c69d")};
    if (palette == QStringLiteral("mono"))
        return {QColor("#ffffff"), QColor("#d9e1e8"), QColor("#aeb8c2"), QColor("#7f8b96"), QColor("#edf2f4"), QColor("#bac4ce")};
    if (palette == QStringLiteral("pastel"))
        return {QColor("#ffc8dd"), QColor("#bde0fe"), QColor("#caffbf"), QColor("#ffd6a5"), QColor("#e7c6ff"), QColor("#a2d2ff")};
    return {QColor("#d9fbff"), QColor("#3dd6e8"), QColor("#3a86ff"), QColor("#7358d6"), QColor("#2aa889"), QColor("#9bf6ff")};
}

bool SnakeSimulation::exportFrame()
{
    snakes_core_frame_sizes sizes{};
    if (!m_world || snakes_core_get_frame_sizes(m_world.get(), &sizes) != SNAKES_CORE_OK) return false;
    grow(m_frame.snakes, sizes.snakes);
    grow(m_frame.segments, sizes.segments);
    grow(m_frame.food, sizes.food);
    return snakes_core_export_frame(m_world.get(), m_frame.snakes.data(), m_frame.snakes.size(),
                                    m_frame.segments.data(), m_frame.segments.size(),
                                    m_frame.food.data(), m_frame.food.size(), &m_frame.info) == SNAKES_CORE_OK;
}

void SnakeSimulation::advance(double deltaSeconds)
{
    if (!m_world || m_paused || !std::isfinite(deltaSeconds) || deltaSeconds <= 0) return;
    m_accumulator += std::min(deltaSeconds, 0.1);
    const auto ticks = uint32_t(std::floor((m_accumulator + 1e-12) / physicsStepSeconds()));
    if (ticks) {
        if (snakes_core_step(m_world.get(), ticks) != SNAKES_CORE_OK) return;
        m_accumulator = std::max(0.0, m_accumulator - ticks * physicsStepSeconds());
        exportFrame();
    }
    Q_EMIT presented();
}

bool SnakeSimulation::resize(double width, double height)
{
    if (!std::isfinite(width) || !std::isfinite(height) || width <= 0 || height <= 0) return false;
    const QSizeF viewSize(width, height);
    width = extent(width); height = extent(height);
    if (width == m_config.width && height == m_config.height) {
        if (viewSize != m_viewSize) {
            m_viewSize = viewSize;
            Q_EMIT presented();
        }
        return true;
    }
    if (!m_world || snakes_core_resize(m_world.get(), width, height) != SNAKES_CORE_OK) return false;
    m_viewSize = viewSize;
    m_config.width = width; m_config.height = height;
    const bool result = exportFrame();
    Q_EMIT presented();
    return result;
}

bool SnakeSimulation::reconfigure(const snakes_core_config &config)
{
    if (!m_world || snakes_core_reconfigure(m_world.get(), &config) != SNAKES_CORE_OK) return false;
    if (config.width != m_config.width || config.height != m_config.height)
        m_viewSize = QSizeF(config.width, config.height);
    m_config = config;
    const bool result = exportFrame();
    if (m_frame.info.tick == 0) m_accumulator = 0;
    Q_EMIT presented();
    return result;
}

void SnakeSimulation::applySettings(const Configuration &settings)
{
    m_palette = colors(settings.animationPalette());
    setPaused(settings.reducedMotion());
    reconfigure(configuration(settings, m_config.width, m_config.height,
                               std::bit_cast<quint32>(m_config.seed)));
}

void SnakeSimulation::setPaused(bool paused)
{
    if (m_paused == paused) return;
    m_paused = paused;
    m_accumulator = 0;
    Q_EMIT presented();
}

void registerSnakeTypes()
{
    static const bool registered = [] {
        qmlRegisterType<SnakeRenderer>("Screensaver.Native", 1, 0, "SnakeRenderer");
        qmlRegisterUncreatableType<SnakeSimulation>("Screensaver.Native", 1, 0, "SnakeSimulation", QStringLiteral("Owned by OverlayManager"));
        return true;
    }();
    Q_UNUSED(registered)
}

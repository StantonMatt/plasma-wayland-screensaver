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
    m_frame = std::make_shared<SnakeFrame>();
    m_frame->events.reserve(SNAKES_CORE_MAX_EVENTS);
    m_storage.push_back(m_frame);
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
            uint32_t(settings.snakeDeadlyWalls()), SNAKES_CORE_RULE_DEFAULT, 0};
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

void SnakeSimulation::clearHistory()
{
    for (auto &frame : m_history) frame.reset();
    m_historyCount = m_historyHead = 0;
}

void SnakeSimulation::setPresentationLead(qint64 nanoseconds)
{
    m_historyLimit = std::clamp(size_t(std::ceil(std::clamp(nanoseconds / 1e9, 0.0, 1.0)
                                               / physicsStepSeconds())) + 2,
                                size_t(2), maximumHistoryFrames);
    // Retain already exported history across hotplug/refresh changes. A lower
    // limit takes effect as old entries expire; increasing it never rewinds time.
}

bool SnakeSimulation::exportFrame()
{
    snakes_core_frame_sizes sizes{};
    if (!m_world || snakes_core_get_frame_sizes(m_world.get(), &sizes) != SNAKES_CORE_OK) return false;
    if (m_historyCount >= m_historyLimit) {
        const size_t oldest = (m_historyHead + maximumHistoryFrames - m_historyCount) % maximumHistoryFrames;
        m_history[oldest].reset();
        --m_historyCount;
    }
    m_frame.reset();
    const auto available = std::find_if(m_storage.begin(), m_storage.end(),
                                       [](const auto &frame) { return frame.use_count() == 1; });
    if (available != m_storage.end()) m_frame = *available;
    else {
        m_frame = std::make_shared<SnakeFrame>();
        // A newly attached/delayed viewport can pin an extra buffer after the
        // world has matured. Start its replacement at the existing high water,
        // rather than growing it again during subsequent steady-state ticks.
        size_t snakes = 0, segments = 0, food = 0;
        for (const auto &frame : m_storage) {
            snakes = std::max(snakes, frame->snakes.capacity());
            segments = std::max(segments, frame->segments.capacity());
            food = std::max(food, frame->food.capacity());
        }
        m_frame->snakes.reserve(snakes);
        m_frame->segments.reserve(segments);
        m_frame->food.reserve(food);
        m_storage.push_back(m_frame);
    }
    grow(m_frame->snakes, sizes.snakes);
    grow(m_frame->segments, sizes.segments);
    grow(m_frame->food, sizes.food);
    m_frame->events.reserve(SNAKES_CORE_MAX_EVENTS);
    grow(m_frame->events, sizes.events);
    grow(m_frame->items, sizes.items);
    if (snakes_core_export_frame(m_world.get(), m_frame->snakes.data(), m_frame->snakes.size(),
                                m_frame->segments.data(), m_frame->segments.size(),
                                m_frame->food.data(), m_frame->food.size(), &m_frame->info) != SNAKES_CORE_OK)
        return false;
    if (snakes_core_export_extras(m_world.get(), m_frame->items.data(), m_frame->items.size(),
                                 m_frame->events.data(), m_frame->events.size()) != SNAKES_CORE_OK)
        return false;
    if (m_presentationHistoryCount) {
        const auto &previous = m_presentationHistory[(m_presentationHistoryHead + maximumHistoryFrames - 1)
                                                     % maximumHistoryFrames].info;
        if (previous.geometry_generation != m_frame->info.geometry_generation || previous.tick > m_frame->info.tick)
            m_presentationHistoryCount = m_presentationHistoryHead = 0;
        else if (previous.tick == m_frame->info.tick) {
            m_presentationHistoryHead = (m_presentationHistoryHead + maximumHistoryFrames - 1) % maximumHistoryFrames;
            --m_presentationHistoryCount; // Replace same-tick settings exports.
        }
    }
    auto &state = m_presentationHistory[m_presentationHistoryHead];
    state.info = m_frame->info;
    state.snakeCount = m_frame->snakes.size();
    state.eventCount = m_frame->events.size();
    state.foodCount = m_frame->food.size();
    for (size_t i = 0; i < state.snakeCount; ++i) {
        auto snake = m_frame->snakes[i];
        if (snake.segment_count) state.tails[i] = m_frame->segments[snake.segment_offset + snake.segment_count - 1];
        snake.segment_offset = uint32_t(i);
        snake.segment_count = snake.segment_count ? 1 : 0;
        state.snakes[i] = snake;
    }
    std::copy(m_frame->events.begin(), m_frame->events.end(), state.events.begin());
    m_presentationHistoryHead = (m_presentationHistoryHead + 1) % maximumHistoryFrames;
    m_presentationHistoryCount = std::min(m_presentationHistoryCount + 1, maximumHistoryFrames);
    m_history[m_historyHead] = m_frame;
    m_historyHead = (m_historyHead + 1) % maximumHistoryFrames;
    ++m_historyCount;
    return true;
}

void SnakeSimulation::advance(double deltaSeconds)
{
    if (!m_world || m_paused || !std::isfinite(deltaSeconds) || deltaSeconds <= 0) return;
    m_accumulator += std::min(deltaSeconds, 0.1);
    clearHistory();
    const auto ticks = uint32_t(std::floor((m_accumulator + 1e-12) / physicsStepSeconds()));
    if (ticks) {
        for (uint32_t tick = 0; tick < ticks; ++tick) {
            if (snakes_core_step(m_world.get(), 1) != SNAKES_CORE_OK) return;
            exportFrame();
        }
        m_accumulator = std::max(0.0, m_accumulator - ticks * physicsStepSeconds());
    }
    Q_EMIT presented();
}

void SnakeSimulation::advanceTo(qint64 presentationNanoseconds)
{
    if (!m_presentationNanoseconds) {
        m_presentationNanoseconds = presentationNanoseconds;
        return;
    }
    if (presentationNanoseconds <= *m_presentationNanoseconds) return;
    const double delta = (presentationNanoseconds - *m_presentationNanoseconds) / 1e9;
    m_presentationNanoseconds = presentationNanoseconds;
    if (!m_world || m_paused) return;
    m_accumulator += std::min(delta, 0.1);
    const auto ticks = uint32_t(std::floor((m_accumulator + 1e-12) / physicsStepSeconds()));
    if (!ticks) return;
    // Export each boundary so all independent display phases can sample it.
    // Every buffer is caller-owned and reused after its last lease expires.
    for (uint32_t tick = 0; tick < ticks; ++tick) {
        if (snakes_core_step(m_world.get(), 1) != SNAKES_CORE_OK) return;
        exportFrame();
    }
    m_accumulator = std::max(0.0, m_accumulator - ticks * physicsStepSeconds());
    // Do not notify every viewport here: only its own clock requests a render.
}

const SnakeFrame &SnakeSimulation::frameAt(qint64 presentationNanoseconds, double &alpha) const
{
    const double offset = m_presentationNanoseconds && !m_paused
        ? (presentationNanoseconds - *m_presentationNanoseconds) / 1e9 : 0;
    const double time = m_frame->info.simulation_time + m_accumulator + offset;
    const SnakeFrame *frame = m_frame.get();
    for (size_t age = 0; age < m_historyCount; ++age) {
        const auto &candidate = m_history[(m_historyHead + maximumHistoryFrames - 1 - age)
                                          % maximumHistoryFrames];
        frame = candidate.get();
        if (frame->info.simulation_time <= time + 1e-12) break;
    }
    alpha = std::clamp((time - frame->info.simulation_time) / physicsStepSeconds(), 0.0, 1.0);
    return *frame;
}

std::shared_ptr<const SnakeFrame> SnakeSimulation::retainFrameAt(qint64 presentationNanoseconds,
                                                               double &alpha) const
{
    const auto *frame = &frameAt(presentationNanoseconds, alpha);
    for (const auto &entry : m_storage) if (entry.get() == frame) return entry;
    return {};
}

void SnakeSimulation::retainPresentationHistory(const snakes_core_frame_info *after,
    const snakes_core_frame_info &through,
    std::array<SnakePresentationFrame, maximumHistoryFrames> &frames,
    size_t &head, size_t &count) const
{
    // Find just the requested suffix. Normal presentation checks one new
    // boundary plus the preceding tick, rather than scanning the entire ring.
    size_t newestAge = 0, oldestAge = 0;
    for (size_t age = 1; age <= m_presentationHistoryCount; ++age) {
        const auto &info = m_presentationHistory[(m_presentationHistoryHead + maximumHistoryFrames - age)
                                                % maximumHistoryFrames].info;
        if (after && info.tick <= after->tick) break;
        if (info.geometry_generation != through.geometry_generation || info.tick > through.tick) continue;
        if (!newestAge) newestAge = age;
        oldestAge = age;
    }
    for (size_t age = oldestAge; age >= newestAge && age > 0; --age) {
        frames[head] = m_presentationHistory[(m_presentationHistoryHead + maximumHistoryFrames - age)
                                            % maximumHistoryFrames];
        head = (head + 1) % maximumHistoryFrames;
        count = std::min(count + 1, maximumHistoryFrames);
    }
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
    clearHistory();
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
    clearHistory();
    const bool result = exportFrame();
    if (m_frame->info.tick == 0) {
        m_accumulator = 0;
        m_presentationNanoseconds.reset();
    }
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
    clearHistory();
    m_presentationNanoseconds.reset();
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

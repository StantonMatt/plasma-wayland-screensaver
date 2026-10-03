// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once

#include <snakes_core.h>
#include <QColor>
#include <QObject>
#include <QSizeF>
#include <QVector>
#include <memory>
#include <array>
#include <optional>
#include <vector>

class Configuration;

// Caller-owned ABI records. Storage grows only when the world's high-water
// count increases, and every viewport reads the same exported history.
struct SnakeFrame {
    std::vector<snakes_core_snake> snakes;
    std::vector<snakes_core_segment> segments;
    std::vector<snakes_core_food> food;
    std::vector<snakes_core_item> items;
    std::vector<snakes_core_event> events;
    snakes_core_frame_info info{};
    double itemRadius = 0;
};

// Compact state consumed by Rust's visual history.
// One tail point per snake preserves boost samples; flags preserve corpse and
// leader transitions. Every event retains its original completed tick.
struct SnakePresentationFrame {
    std::array<snakes_core_snake, SNAKES_CORE_MAX_SNAKES> snakes{};
    std::array<snakes_core_segment, SNAKES_CORE_MAX_SNAKES> tails{};
    std::array<snakes_core_event, SNAKES_CORE_MAX_EVENTS> events{};
    snakes_core_frame_info info{};
    size_t snakeCount = 0, eventCount = 0, foodCount = 0;
};

class SnakeSimulation final : public QObject
{
    Q_OBJECT
    friend class SnakeSimulationTest;
    Q_PROPERTY(double simulationTime READ simulationTime NOTIFY presented)
    Q_PROPERTY(double physicsStepSeconds READ physicsStepSeconds CONSTANT)
    Q_PROPERTY(double interpolation READ interpolation NOTIFY presented)
public:
    // GUI-thread owned, including the Rust world, pool, leases and history.
    // Renderers capture immutable frames/history on the GUI thread; only their
    // updatePaintNode consumes those snapshots while Qt blocks the GUI thread.
    explicit SnakeSimulation(const snakes_core_config &config, QObject *parent = nullptr);
    ~SnakeSimulation() override;
    static snakes_core_config configuration(const Configuration &settings, double width,
                                             double height, quint32 seed);
    static QVector<QColor> colors(const QString &palette);
    const SnakeFrame &frame() const { return *m_frame; }
    std::shared_ptr<const SnakeFrame> retainFrame() const { return m_frame; }
    const snakes_core_config &config() const { return m_config; }
    // Logical overlay extent, before enforcing the C ABI's arena limits.
    QSizeF viewSize() const { return m_viewSize; }
    const QVector<QColor> &palette() const { return m_palette; }
    double simulationTime() const { return m_frame->info.simulation_time; }
    static constexpr double physicsStepSeconds() { return 1.0 / 30.0; }
    double interpolation() const { return m_accumulator / physicsStepSeconds(); }
    void advance(double deltaSeconds);
    // Requests from all view clocks share a high-water presentation timeline.
    // Older/duplicate requests interpolate history without stepping again.
    void advanceTo(qint64 presentationNanoseconds);
    const SnakeFrame &frameAt(qint64 presentationNanoseconds, double &alpha) const;
    // Immutable leases pin pending render requests independently of history.
    std::shared_ptr<const SnakeFrame> retainFrameAt(qint64 presentationNanoseconds, double &alpha) const;
    void setPresentationLead(qint64 nanoseconds);
    static constexpr size_t maximumHistoryFrames = 32;
    // Append intervening visual state, oldest first. This compact fixed ring
    // covers every effect lifetime independently of display phase history.
    void retainPresentationHistory(const snakes_core_frame_info *after,
        const snakes_core_frame_info &through,
        std::array<SnakePresentationFrame, maximumHistoryFrames> &frames,
        size_t &head, size_t &count) const;
    bool resize(double width, double height);
    bool reconfigure(const snakes_core_config &config);
    void applySettings(const Configuration &settings);
    void setPaused(bool paused);
    bool isValid() const { return bool(m_world); }
Q_SIGNALS:
    void presented();
private:
    bool exportFrame();
    std::unique_ptr<snakes_core_world, decltype(&snakes_core_destroy)> m_world{nullptr, snakes_core_destroy};
    snakes_core_config m_config{};
    QSizeF m_viewSize;
    // The pacing clock predicts at most one refresh ahead (refresh >= 1 Hz).
    // ceil(1 second / physicsStep) + two boundary frames covers every phase.
    // Storage is reused only when neither history nor a renderer leases it;
    // pool size is bounded by history high water plus pending viewport count.
    std::shared_ptr<SnakeFrame> m_frame;
    std::array<std::shared_ptr<SnakeFrame>, maximumHistoryFrames> m_history;
    std::vector<std::shared_ptr<SnakeFrame>> m_storage;
    // Separate from display phase history: hotplug can shrink the latter to
    // two frames, but transient effects must retain their completed ticks.
    std::array<SnakePresentationFrame, maximumHistoryFrames> m_presentationHistory;
    size_t m_presentationHistoryCount = 0, m_presentationHistoryHead = 0;
    size_t m_historyCount = 0;
    size_t m_historyHead = 0;
    size_t m_historyLimit = maximumHistoryFrames;
    std::optional<qint64> m_presentationNanoseconds;
    void clearHistory();
    QVector<QColor> m_palette = colors(QStringLiteral("ocean"));
    double m_accumulator = 0;
    bool m_paused = false;
};

void registerSnakeTypes();

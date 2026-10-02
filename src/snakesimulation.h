// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once

#include <snakes_core.h>
#include <QColor>
#include <QObject>
#include <QSizeF>
#include <QVector>
#include <memory>
#include <vector>

class Configuration;

// Caller-owned ABI records. Storage grows only when the world's high-water
// count increases, and every viewport reads the same exported frame.
struct SnakeFrame {
    std::vector<snakes_core_snake> snakes;
    std::vector<snakes_core_segment> segments;
    std::vector<snakes_core_food> food;
    snakes_core_frame_info info{};
};

class SnakeSimulation final : public QObject
{
    Q_OBJECT
    Q_PROPERTY(double simulationTime READ simulationTime NOTIFY presented)
    Q_PROPERTY(double physicsStepSeconds READ physicsStepSeconds CONSTANT)
    Q_PROPERTY(double interpolation READ interpolation NOTIFY presented)
public:
    explicit SnakeSimulation(const snakes_core_config &config, QObject *parent = nullptr);
    ~SnakeSimulation() override;
    static snakes_core_config configuration(const Configuration &settings, double width,
                                             double height, quint32 seed);
    static QVector<QColor> colors(const QString &palette);
    const SnakeFrame &frame() const { return m_frame; }
    const snakes_core_config &config() const { return m_config; }
    // Logical overlay extent, before enforcing the C ABI's arena limits.
    QSizeF viewSize() const { return m_viewSize; }
    const QVector<QColor> &palette() const { return m_palette; }
    double simulationTime() const { return m_frame.info.simulation_time; }
    static constexpr double physicsStepSeconds() { return 1.0 / 30.0; }
    double interpolation() const { return m_accumulator / physicsStepSeconds(); }
    void advance(double deltaSeconds);
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
    SnakeFrame m_frame;
    QVector<QColor> m_palette = colors(QStringLiteral("ocean"));
    double m_accumulator = 0;
    bool m_paused = false;
};

void registerSnakeTypes();

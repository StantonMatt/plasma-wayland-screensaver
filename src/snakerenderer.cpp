// SPDX-License-Identifier: GPL-3.0-or-later
#include "snakerenderer.h"
#include "snakematerial.h"
#include <QQuickWindow>
#include <QSGRendererInterface>

#include <QSGGeometry>
#include <QSGGeometryNode>
#include <QSGVertexColorMaterial>
#include <algorithm>
#include <limits>
#include <cstring>

namespace {
using Vertex = QSGGeometry::ColoredPoint2D;
// Zero-scale builds consume only food's count for LOD hysteresis. Supply real,
// initialized records for the borrowed slice without copying historical food.
const std::array<snakes_core_food, SNAKES_CORE_MAX_FOOD> historyFood{};
static_assert(sizeof(Vertex) == sizeof(snakes_core_render_vertex));
static_assert(alignof(Vertex) == alignof(snakes_core_render_vertex));
static_assert(offsetof(Vertex, x) == offsetof(snakes_core_render_vertex, x));
static_assert(offsetof(Vertex, y) == offsetof(snakes_core_render_vertex, y));
static_assert(offsetof(Vertex, r) == offsetof(snakes_core_render_vertex, color));
static_assert(offsetof(Vertex, g) == offsetof(snakes_core_render_vertex, color) + 1);
static_assert(offsetof(Vertex, b) == offsetof(snakes_core_render_vertex, color) + 2);
static_assert(offsetof(Vertex, a) == offsetof(snakes_core_render_vertex, color) + 3);
qreal clamped(qreal value, qreal minimum, qreal maximum)
{
    return std::max(minimum, std::min(maximum, value));
}
struct GeometryNode : QSGGeometryNode {
    std::unique_ptr<snakes_core_renderer, decltype(&snakes_core_render_destroy)> renderer{
        snakes_core_render_create(), snakes_core_render_destroy};
    bool shader = false;
    quint64 epoch = 0;
    std::optional<snakes_core_frame_info> historyThrough;
};
}

SnakeRenderer::SnakeRenderer(QQuickItem *parent)
    : QQuickItem(parent)
{
    setFlag(ItemHasContents, true);
    const auto observeWindow = [this](QQuickWindow *window) {
        disconnect(m_shaderErrorConnection);
        m_shaderFailed = false;
        if (window) m_shaderErrorConnection = connect(window, &QQuickWindow::sceneGraphError, this,
            [this](QQuickWindow::SceneGraphError, const QString &) { m_shaderFailed = true; update(); });
    };
    connect(this, &QQuickItem::windowChanged, this, observeWindow);
    observeWindow(window());
}

void SnakeRenderer::setSimulation(SnakeSimulation *simulation)
{
    if (m_simulation == simulation) return;
    disconnect(m_presentedConnection);
    disconnect(m_destroyedConnection);
    m_simulation = simulation;
    ++m_renderEpoch;
    m_pendingHistoryCount = m_pendingHistoryHead = 0;
    m_capturedThrough.reset();
    m_frame = nullptr;
    m_retainedFrame.reset();
    m_presentationNanoseconds.reset();
    if (simulation) {
        const auto sync = [this, simulation] {
            syncFrame(simulation->frame(), simulation->palette(), simulation->interpolation(),
                      simulation->config().deadly_walls);
            m_retainedFrame = simulation->retainFrame();
        };
        m_presentedConnection = connect(simulation, &SnakeSimulation::presented, this, sync);
        m_destroyedConnection = connect(simulation, &QObject::destroyed, this, [this] {
            ++m_renderEpoch;
            m_pendingHistoryCount = m_pendingHistoryHead = 0;
            m_capturedThrough.reset();
            m_frame = nullptr;
            m_retainedFrame.reset();
            m_presentationNanoseconds.reset();
            update();
            Q_EMIT simulationChanged();
        });
        sync();
    }
    update();
    Q_EMIT simulationChanged();
}

void SnakeRenderer::syncFrame(const SnakeFrame &frame, const QVector<QColor> &palette,
                              qreal interpolation, bool deadlyWalls)
{
    m_retainedFrame.reset();
    m_presentationNanoseconds.reset();
    loadFrame(frame, palette, interpolation, deadlyWalls);
    update();
}

void SnakeRenderer::loadFrame(const SnakeFrame &frame, const QVector<QColor> &palette,
                              qreal interpolation, bool deadlyWalls)
{
    m_frame = &frame;
    if (m_palette.constData() != palette.constData()) {
        m_palette = palette;
        m_renderPalette.resize(palette.size());
        for (qsizetype i = 0; i < palette.size(); ++i) {
            const QRgb color = palette[i].rgba();
            m_renderPalette[i] = {uchar(qRed(color)), uchar(qGreen(color)),
                                  uchar(qBlue(color)), uchar(qAlpha(color))};
        }
    }
    m_simulationTime = frame.info.simulation_time;
    m_interpolation = clamped(interpolation, 0.0, 1.0);
    m_shaderTime = m_simulationTime + m_interpolation * SnakeSimulation::physicsStepSeconds();
    m_worldWidth = std::max(1.0, frame.info.world_width);
    m_worldHeight = std::max(1.0, frame.info.world_height);
    m_worldToViewX = m_simulation ? m_simulation->viewSize().width() / m_worldWidth : 1.0;
    m_worldToViewY = m_simulation ? m_simulation->viewSize().height() / m_worldHeight : 1.0;
    m_deadlyWalls = deadlyWalls;
    captureHistory();
}

void SnakeRenderer::captureHistory()
{
    if (!m_simulation || !m_frame) return;
    const auto &info = m_frame->info;
    if (m_capturedThrough && (info.geometry_generation != m_capturedThrough->geometry_generation
                             || info.tick < m_capturedThrough->tick)) {
        m_pendingHistoryCount = m_pendingHistoryHead = 0;
        m_capturedThrough.reset();
    }
    if (m_capturedThrough && info.tick == m_capturedThrough->tick) return;
    m_simulation->retainPresentationHistory(m_capturedThrough ? &*m_capturedThrough : nullptr,
                                           info, m_pendingHistory, m_pendingHistoryHead, m_pendingHistoryCount);
    m_capturedThrough = info;
}

void SnakeRenderer::presentAt(qint64 presentationNanoseconds)
{
    if (!m_simulation) return;
    m_presentationNanoseconds = presentationNanoseconds;
    double alpha = 0;
    m_retainedFrame = m_simulation->retainFrameAt(presentationNanoseconds, alpha);
    loadFrame(*m_retainedFrame, m_simulation->palette(), alpha, m_simulation->config().deadly_walls);
    m_simulationTime += alpha * SnakeSimulation::physicsStepSeconds();
    m_shaderTime = m_simulationTime;
    update();
}

void SnakeRenderer::presentFrame(qreal simulationTime, qreal interpolation)
{
    m_simulationTime = simulationTime;
    m_shaderTime = simulationTime;
    m_interpolation = clamped(interpolation, 0.0, 1.0);
    update();
}

void SnakeRenderer::setDrawOffset(qreal drawOffsetX, qreal drawOffsetY)
{
    if (qFuzzyCompare(m_drawOffsetX, drawOffsetX)
            && qFuzzyCompare(m_drawOffsetY, drawOffsetY)) {
        return;
    }
    m_drawOffsetX = drawOffsetX;
    m_drawOffsetY = drawOffsetY;
    update();
}

void SnakeRenderer::setScaleToViewport(bool scaleToViewport)
{
    if (m_scaleToViewport == scaleToViewport) {
        return;
    }
    m_scaleToViewport = scaleToViewport;
    update();
}

void SnakeRenderer::setDeveloperMode(bool enabled)
{
    if (m_developerMode == enabled) {
        return;
    }
    m_developerMode = enabled;
    update();
}

void SnakeRenderer::setShaderTimeFrozen(bool frozen)
{
    if (m_shaderTimeFrozen == frozen) return;
    if (frozen) m_frozenShaderTime = m_shaderTime;
    m_shaderTimeFrozen = frozen;
    update();
}

QSGNode *SnakeRenderer::updatePaintNode(QSGNode *oldNode,
                                        UpdatePaintNodeData *updatePaintNodeData)
{
    Q_UNUSED(updatePaintNodeData)
    // Each scene-graph node owns Rust scratch/history for this window. Immutable
    // frame leases keep GUI snapshots alive until scene-graph sync completes.
    auto *node = static_cast<GeometryNode *>(oldNode);
    // Resource decoding and driver compilation occur only on node creation.
    // Keep the decision with the node: no capability checks or allocations per
    // presentation, and recreated scene graphs probe the current device again.
    bool shader = node ? node->shader && !m_shaderFailed : false;
    if (!node && !m_shaderFailed) {
        shader = m_shaderGeometryForTest ? SnakeMaterial::shadersAvailable()
            : (m_shaderSupportCheck ? m_shaderSupportCheck(window()) : SnakeMaterial::supportsWindow(window()));
        if (window() && QSGRendererInterface::isApiRhiBased(window()->rendererInterface()->graphicsApi())
            && !shader) m_shaderFailed = true;
    }
    m_shaderInUse = shader;
    if (node && node->shader != shader) { delete node; node = nullptr; }
    if (!node) {
        node = new GeometryNode;
        node->shader = shader;
        auto *geometry = new QSGGeometry(shader ? SnakeMaterial::attributes() : QSGGeometry::defaultAttributes_ColoredPoint2D(), 0);
        geometry->setDrawingMode(QSGGeometry::DrawTriangles);
        geometry->setVertexDataPattern(QSGGeometry::DynamicPattern);
        node->setGeometry(geometry);
        node->setFlag(QSGNode::OwnsGeometry);
        QSGMaterial *material = shader ? static_cast<QSGMaterial *>(new SnakeMaterial) : new QSGVertexColorMaterial;
        material->setFlag(QSGMaterial::Blending, true);
        node->setMaterial(material);
        node->setFlag(QSGNode::OwnsMaterial);
        m_geometryCapacity = 0;
    }
    // Freeze all presentation-time animation at the same boundary for both
    // geometry formats, including transient ages and discrete head animation.
    snakes_core_render_set_reduced_motion(node->renderer.get(), uint32_t(m_shaderTimeFrozen));
    snakes_core_render_set_clock_rect(node->renderer.get(), m_clockRect.x(), m_clockRect.y(),
                                     qMax(0.0, m_clockRect.width()), qMax(0.0, m_clockRect.height()));
    const qreal animationTime = m_shaderTimeFrozen ? m_frozenShaderTime : m_shaderTime;
    if (shader) {
        auto *material = static_cast<SnakeMaterial *>(node->material());
        material->time = float(animationTime);
        material->animationTime = float(animationTime);
        material->motionScale = m_shaderTimeFrozen ? 0.6f : 1.0f;
        material->paletteMode = !m_palette.isEmpty() && m_palette.first() == QColor(255, 255, 255) ? 1.0f
            : !m_palette.isEmpty() && m_palette.first() == QColor(255, 200, 221) ? 2.0f : 0.0f;
        node->markDirty(QSGNode::DirtyMaterial);
    }
    if (node->epoch != m_renderEpoch) {
        snakes_core_render_reset(node->renderer.get());
        node->historyThrough.reset();
        node->epoch = m_renderEpoch;
    }
    const auto &info = m_frame ? m_frame->info : snakes_core_frame_info{0, 0, 1, 1, 0, 1, 0, {}, {}};
    const snakes_core_render_params params{
        width(), height(),
        m_scaleToViewport ? width() / m_worldWidth : m_worldToViewX,
        m_scaleToViewport ? height() / m_worldHeight : m_worldToViewY,
        m_drawOffsetX, m_drawOffsetY, m_interpolation,
        m_shaderTimeFrozen ? m_frozenShaderTime : m_simulationTime,
        uint32_t(m_deadlyWalls), uint32_t(m_developerMode)};
    // GUI-side compact copies keep intermediate ticks until scene-graph sync. Feed
    // history without tessellation: zero scale exits after consuming history.
    // Kill flashes, corpse fade starts and boost trail samples keep their
    // original physics timestamps. The final frame is consumed by build below.
    auto shaderParams = params;
    shaderParams.presentation_time = animationTime;
    auto historyParams = shader ? shaderParams : params;
    historyParams.scale_x = historyParams.scale_y = 0;
    // Keep the compact ring after sync so a recreated scene graph can restore
    // effect ages. Existing nodes scan only new boundaries (usually none/one).
    size_t replayCount = 0;
    for (size_t age = 1; age <= m_pendingHistoryCount; ++age) {
        const auto &frame = m_pendingHistory[(m_pendingHistoryHead + m_pendingHistory.size() - age)
                                             % m_pendingHistory.size()];
        if (node->historyThrough && node->historyThrough->geometry_generation == info.geometry_generation
            && node->historyThrough->tick <= info.tick && frame.info.tick <= node->historyThrough->tick) break;
        ++replayCount;
    }
    for (size_t age = replayCount; age > 0; --age) {
        const auto &frame = m_pendingHistory[(m_pendingHistoryHead + m_pendingHistory.size() - age)
                                             % m_pendingHistory.size()];
        if (frame.info.tick < info.tick) {
            snakes_core_render_output ignored{};
            // History calls have no output: both formats accept initialized
            // compact records and update their own effect state only.
            if (shader) snakes_core_render_build_shader(node->renderer.get(), &frame.info,
                frame.snakes.data(), frame.snakeCount, frame.tails.data(), frame.snakeCount,
                historyFood.data(), frame.foodCount, frame.events.data(), frame.eventCount, nullptr, 0,
                &historyParams, nullptr, 0, &ignored);
            else snakes_core_render_build(node->renderer.get(), &frame.info,
                frame.snakes.data(), frame.snakeCount, frame.tails.data(), frame.snakeCount,
                historyFood.data(), frame.foodCount, frame.events.data(), frame.eventCount, nullptr, 0,
                &historyParams, nullptr, 0, &ignored);
        }
    }
    auto *geometry = node->geometry();
    snakes_core_render_set_items(node->renderer.get(),
        m_frame ? m_frame->items.data() : nullptr, m_frame ? m_frame->items.size() : 0, m_frame ? m_frame->itemRadius : 0);
    snakes_core_render_output result{};
    const auto build = [&] {
        if (shader) return snakes_core_render_build_shader(node->renderer.get(), &info,
            m_frame ? m_frame->snakes.data() : nullptr, m_frame ? m_frame->snakes.size() : 0,
            m_frame ? m_frame->segments.data() : nullptr, m_frame ? m_frame->segments.size() : 0,
            m_frame ? m_frame->food.data() : nullptr, m_frame ? m_frame->food.size() : 0,
            m_frame ? m_frame->events.data() : nullptr, m_frame ? m_frame->events.size() : 0,
            m_renderPalette.data(), m_renderPalette.size(), &shaderParams,
            static_cast<snakes_core_shader_vertex *>(geometry->vertexData()), m_geometryCapacity, &result);
        return snakes_core_render_build(node->renderer.get(), &info,
            m_frame ? m_frame->snakes.data() : nullptr, m_frame ? m_frame->snakes.size() : 0,
            m_frame ? m_frame->segments.data() : nullptr, m_frame ? m_frame->segments.size() : 0,
            m_frame ? m_frame->food.data() : nullptr, m_frame ? m_frame->food.size() : 0,
            m_frame ? m_frame->events.data() : nullptr, m_frame ? m_frame->events.size() : 0,
            m_renderPalette.data(), m_renderPalette.size(), &params,
            reinterpret_cast<snakes_core_render_vertex *>(geometry->vertexData()),
            m_geometryCapacity, &result);
    };
    int status = build();
    if (status == SNAKES_CORE_BUFFER_TOO_SMALL && result.vertex_count <= size_t(std::numeric_limits<int>::max())) {
        // The Rust call reports its full count even with no/insufficient space.
        // Grow once and retry; history/effects are deduplicated by frame tick.
        const qint64 grownCapacity = m_geometryCapacity > 0
            ? qint64(m_geometryCapacity) + std::max(1024, m_geometryCapacity / 2) : 4096;
        m_geometryCapacity = int(std::min<qint64>(std::numeric_limits<int>::max(),
            std::max<qint64>(result.vertex_count, grownCapacity)));
        geometry->allocate(m_geometryCapacity);
        // Qt leaves malloc-backed vertex storage uninitialized. Rust borrows
        // every slot as a Vertex, so initialize the entire capacity once per
        // allocation, including the unused tail. No steady-state clearing.
        std::memset(geometry->vertexData(), 0, size_t(m_geometryCapacity) * geometry->sizeOfVertex());
        status = build();
    }
    if (status == SNAKES_CORE_OK || status == SNAKES_CORE_BUFFER_TOO_SMALL) node->historyThrough = info;
    const int count = status == SNAKES_CORE_OK ? int(result.vertex_count) : 0;
    m_denseFoodRendering = result.dense_food;
#if QT_VERSION >= QT_VERSION_CHECK(6, 10, 0)
    geometry->setVertexCount(count);
#else
    // Qt 6.8/6.9 lack setVertexCount. Rebuild after reallocating only when the
    // count changes; current Qt writes directly into the retained GPU buffer.
    if (m_geometryCapacity != count) {
        geometry->allocate(count);
        if (count) std::memset(geometry->vertexData(), 0, size_t(count) * geometry->sizeOfVertex());
        m_geometryCapacity = count;
        if (count) build();
    }
#endif
    geometry->markVertexDataDirty();
    node->markDirty(QSGNode::DirtyGeometry);
    return node;
}

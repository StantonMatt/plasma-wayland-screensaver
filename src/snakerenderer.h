// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include "snakesimulation.h"
#include <QMetaObject>
#include <QPointer>
#include <QQuickItem>

class SnakeRenderer : public QQuickItem
{
    Q_OBJECT
    Q_PROPERTY(SnakeSimulation *simulation READ simulation WRITE setSimulation NOTIFY simulationChanged)
    Q_PROPERTY(bool scaleToViewport READ scaleToViewport WRITE setScaleToViewport)
    Q_PROPERTY(bool shaderTimeFrozen READ shaderTimeFrozen WRITE setShaderTimeFrozen)
    Q_PROPERTY(int season READ season WRITE setSeason)
    Q_PROPERTY(QRectF clockRect READ clockRect WRITE setClockRect)
    Q_PROPERTY(bool developerMode READ developerMode WRITE setDeveloperMode)
    Q_PROPERTY(qreal drawOffsetX READ drawOffsetX WRITE setDrawOffsetX)
    Q_PROPERTY(qreal drawOffsetY READ drawOffsetY WRITE setDrawOffsetY)
    friend class SnakeRendererTest;
public:
    explicit SnakeRenderer(QQuickItem *parent = nullptr);
    SnakeSimulation *simulation() const { return m_simulation; }
    void setSimulation(SnakeSimulation *simulation);
    // The frame must outlive this item (or the next sync). Qt blocks the GUI
    // thread while updatePaintNode reads it on the scene-graph thread.
    void syncFrame(const SnakeFrame &frame, const QVector<QColor> &palette,
                   qreal interpolation, bool deadlyWalls);
    void presentFrame(qreal simulationTime, qreal interpolation);
    void presentAt(qint64 presentationNanoseconds);
    void setDrawOffset(qreal x, qreal y);
    qreal drawOffsetX() const { return m_drawOffsetX; }
    qreal drawOffsetY() const { return m_drawOffsetY; }
    void setDrawOffsetX(qreal x) { setDrawOffset(x, m_drawOffsetY); }
    void setDrawOffsetY(qreal y) { setDrawOffset(m_drawOffsetX, y); }
    bool scaleToViewport() const { return m_scaleToViewport; }
    void setScaleToViewport(bool enabled);
    bool developerMode() const { return m_developerMode; }
    void setDeveloperMode(bool enabled);
    int season() const { return m_season; }
    void setSeason(int season) { season = season == 1 ? 1 : 0; if (m_season == season) return; m_season = season; update(); }
    QRectF clockRect() const { return m_clockRect; }
    void setClockRect(const QRectF &rect) { if (m_clockRect == rect) return; m_clockRect = rect; update(); }
    bool shaderTimeFrozen() const { return m_shaderTimeFrozen; }
    void setShaderTimeFrozen(bool frozen);
Q_SIGNALS:
    void simulationChanged();
protected:
    QSGNode *updatePaintNode(QSGNode *oldNode, UpdatePaintNodeData *data) override;
private:
    // Setters/presentFrame/presentAt and snapshot capture run on the GUI thread.
    // updatePaintNode reads these members (and writes its diagnostics/capacity)
    // only during Qt's synchronization barrier, with the GUI thread blocked.
    // After sync, rendering uses GeometryNode/material-owned copies exclusively;
    // neither the simulation nor this item's state is accessed by draw callbacks.
    void loadFrame(const SnakeFrame &frame, const QVector<QColor> &palette,
                   qreal interpolation, bool deadlyWalls);
    void captureHistory();
    std::array<SnakePresentationFrame, SnakeSimulation::maximumHistoryFrames> m_pendingHistory;
    size_t m_pendingHistoryCount = 0, m_pendingHistoryHead = 0;
    std::optional<snakes_core_frame_info> m_capturedThrough;
    const SnakeFrame *m_frame = nullptr;
    std::shared_ptr<const SnakeFrame> m_retainedFrame;
    QPointer<SnakeSimulation> m_simulation;
    std::optional<qint64> m_presentationNanoseconds;
    QMetaObject::Connection m_presentedConnection;
    QMetaObject::Connection m_destroyedConnection;
    QVector<QColor> m_palette;
    std::vector<snakes_core_render_color> m_renderPalette;
    quint64 m_renderEpoch = 0;
    qreal m_simulationTime = 0;
    qreal m_interpolation = 0;
    qreal m_worldWidth = 1;
    qreal m_worldHeight = 1;
    qreal m_worldToViewX = 1;
    qreal m_worldToViewY = 1;
    QRectF m_clockRect;
    int m_season = 0;
    qreal m_drawOffsetX = 0;
    qreal m_drawOffsetY = 0;
    bool m_deadlyWalls = true;
    bool m_scaleToViewport = false;
    bool m_developerMode = false;
    bool m_denseFoodRendering = false;
    int m_geometryCapacity = 0;
    // A null-window unit fixture can select shader geometry without creating RHI.
    bool m_shaderGeometryForTest = false;
    bool m_shaderTimeFrozen = false;
    bool m_shaderFailed = false;
    bool m_shaderInUse = false;
    // Allows the GPU fallback fixture to probe a deliberately invalid program.
    bool (*m_shaderSupportCheck)(QQuickWindow *) = nullptr;
    qreal m_shaderTime = 0;
    qreal m_frozenShaderTime = 0;
    QMetaObject::Connection m_shaderErrorConnection;
};

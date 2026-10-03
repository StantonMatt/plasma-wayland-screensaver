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
Q_SIGNALS:
    void simulationChanged();
protected:
    QSGNode *updatePaintNode(QSGNode *oldNode, UpdatePaintNodeData *data) override;
private:
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
    qreal m_drawOffsetX = 0;
    qreal m_drawOffsetY = 0;
    bool m_deadlyWalls = true;
    bool m_scaleToViewport = false;
    bool m_developerMode = false;
    bool m_denseFoodRendering = false;
    int m_geometryCapacity = 0;
};

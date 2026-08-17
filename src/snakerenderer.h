// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once

#include <QColor>
#include <QJSValue>
#include <QMetaObject>
#include <QPointF>
#include <QPointer>
#include <QQuickItem>
#include <QVector>

class SnakeRenderer final : public QQuickItem
{
    Q_OBJECT

    friend class SnakeRendererTest;

public:
    explicit SnakeRenderer(QQuickItem *parent = nullptr);

    Q_INVOKABLE void syncFrame(const QJSValue &snakes, const QJSValue &food,
                               const QJSValue &palette, qreal simulationTime,
                               qreal interpolation, qreal worldWidth, qreal worldHeight,
                               qreal drawOffsetX, qreal drawOffsetY, bool deadlyWalls);
    Q_INVOKABLE void presentFrame(qreal simulationTime, qreal interpolation);
    void setDrawOffset(qreal drawOffsetX, qreal drawOffsetY);
    void setScaleToViewport(bool scaleToViewport);
    void setDeveloperMode(bool enabled);
    void follow(SnakeRenderer *source);

Q_SIGNALS:
    void frameSynchronized();

protected:
    QSGNode *updatePaintNode(QSGNode *oldNode,
                             UpdatePaintNodeData *updatePaintNodeData) override;

private:
    struct Segment {
        QPointF position;
        QPointF previous;
    };
    struct Snake {
        QVector<Segment> segments;
        qreal radius = 1.0;
        qreal angle = 0.0;
        qreal desiredAngle = 0.0;
        int colorIndex = 0;
        QVector<int> foodPathIds;
        QVector<QPointF> plannedPath;
        bool alive = false;
    };
    struct Food {
        QPointF position;
        QPointF attractionTarget;
        qreal size = 1.0;
        qreal phase = 0.0;
        qreal attraction = 0.0;
        int colorIndex = 0;
        int id = 0;
    };

    QVector<Snake> m_snakes;
    QVector<Food> m_food;
    QVector<QColor> m_palette;
    qreal m_simulationTime = 0.0;
    qreal m_interpolation = 0.0;
    qreal m_worldWidth = 1.0;
    qreal m_worldHeight = 1.0;
    qreal m_drawOffsetX = 0.0;
    qreal m_drawOffsetY = 0.0;
    bool m_deadlyWalls = true;
    bool m_scaleToViewport = false;
    bool m_developerMode = false;
    bool m_denseFoodRendering = false;
    int m_geometryCapacity = 0;
    QPointer<SnakeRenderer> m_source;
    QMetaObject::Connection m_sourceConnection;
};

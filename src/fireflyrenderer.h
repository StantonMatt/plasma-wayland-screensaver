// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once

#include <QQuickItem>
#include <QString>

class FireflyRenderer final : public QQuickItem
{
    Q_OBJECT

    friend class FireflyRendererTest;

public:
    explicit FireflyRenderer(QQuickItem *parent = nullptr);

    Q_INVOKABLE void configure(int density, qreal sizeScale, qreal motionSpeed,
                               qreal glowAmount, const QString &paletteName, int seed);
    Q_INVOKABLE void presentFrame(qreal phase);

protected:
    QSGNode *updatePaintNode(QSGNode *oldNode,
                             UpdatePaintNodeData *updatePaintNodeData) override;

private:
    int m_density = 50;
    qreal m_sizeScale = 1.0;
    qreal m_motionSpeed = 1.0;
    qreal m_glowAmount = 0.35;
    QString m_paletteName = QStringLiteral("ember");
    int m_seed = 1;
    qreal m_phase = 0.0;
};

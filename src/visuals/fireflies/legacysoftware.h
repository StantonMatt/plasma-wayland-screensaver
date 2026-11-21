// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once

#include <QHash>
#include <QImage>
#include <QPainter>
#include <QQuickWindow>
#include <QSGOpacityNode>
#include <QSGSimpleTextureNode>
#include <cmath>

// Qt Quick's software adaptation cannot paint custom vertex-colour geometry.
// Keep the released renderer's exact disc list and share tiny radial sprites.
class LegacyFireflySoftwareNode final : public QSGNode {
public:
    explicit LegacyFireflySoftwareNode(QQuickWindow* window)
        : m_window(window)
    {
    }
    ~LegacyFireflySoftwareNode() override
    {
        while (auto* child = firstChild())
            delete child;
        qDeleteAll(m_sprites);
    }
    void begin() { m_used = 0; }
    void disc(QPointF centre, qreal radius, const QColor& colour, int peak, int edge)
    {
        if (radius <= 0. || peak <= 0)
            return;
        if (m_used == m_discs.size()) {
            auto* opacity = new QSGOpacityNode;
            auto* image = new QSGSimpleTextureNode;
            image->setFiltering(QSGTexture::Linear);
            opacity->appendChildNode(image);
            appendChildNode(opacity);
            m_discs.append(opacity);
        }
        auto* opacity = m_discs[m_used++];
        auto* image = static_cast<QSGSimpleTextureNode*>(opacity->firstChild());
        const int rim = qRound(255. * edge / peak);
        const quint64 key = (quint64(colour.rgb()) << 8) | uint(rim);
        auto* texture = m_sprites.value(key);
        if (!texture) {
            QImage sprite(96, 96, QImage::Format_ARGB32_Premultiplied);
            sprite.fill(Qt::transparent);
            QPainter painter(&sprite);
            painter.setRenderHint(QPainter::Antialiasing);
            const QRectF bounds(1, 1, 94, 94);
            QRadialGradient gradient(bounds.center(), 47);
            QColor outer = colour;
            outer.setAlpha(rim);
            gradient.setColorAt(0, colour);
            gradient.setColorAt(1, outer);
            painter.setPen(Qt::NoPen);
            painter.setBrush(gradient);
            painter.drawEllipse(bounds);
            painter.end();
            texture = m_window->createTextureFromImage(sprite);
            if (!texture) {
                opacity->setOpacity(0);
                return;
            }
            m_sprites.insert(key, texture);
        }
        if (image->texture() != texture)
            image->setTexture(texture);
        opacity->setOpacity(peak / 255.);
        const qreal extent = radius * (48. / 47.);
        image->setRect(centre.x() - extent, centre.y() - extent, 2 * extent, 2 * extent);
    }
    void finish()
    {
        while (m_discs.size() > m_used)
            delete m_discs.takeLast();
    }

private:
    QQuickWindow* m_window;
    QHash<quint64, QSGTexture*> m_sprites;
    QVector<QSGOpacityNode*> m_discs;
    qsizetype m_used = 0;
};

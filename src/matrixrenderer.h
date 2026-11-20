// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include "visuals_matrix.h"
#include "matrixmaterial.h"
#include <QQuickItem>
#include <QRawFont>
#include <memory>
#include <optional>
#include <vector>
class MatrixRenderer final : public QQuickItem {
    Q_OBJECT
    friend class MatrixRendererTest;
    friend class OverlaySnakesTest;
public:
    explicit MatrixRenderer(QQuickItem *parent=nullptr);
    Q_INVOKABLE void configureMatrix(int density,qreal scale,qreal speed,qreal trails,
        const QString &palette,int seed,bool reduced,double epochMs);
    Q_INVOKABLE void setViewport(const QRectF &,const QPointF &) {} // Preserve local procedural mapping in every monitor mode.
    Q_INVOKABLE void presentAt(qint64 ns);
protected:
    QSGNode *updatePaintNode(QSGNode *,UpdatePaintNodeData *) override;
    void geometryChange(const QRectF &,const QRectF &) override;
private:
    void rebuildLayout();
    void capture(bool advance);
    void buildAtlas();
    void buildVertices();
    struct Glyph { QRawFont font; quint32 index; };
    struct Quad { double x,y; size_t glyph; uint32_t column; float color[4]; };
    std::vector<Glyph> m_glyphs;
    std::vector<Quad> m_quads;
    double m_atlasBaseline=0;
    int m_atlasRows=0;
    std::unique_ptr<visuals_matrix,decltype(&visuals_matrix_destroy)> m_core{nullptr,visuals_matrix_destroy};
    visuals_core_config m_config{};
    visuals_matrix_layout m_layout{};
    std::vector<visuals_matrix_column> m_columns;
    std::vector<float> m_offsets;
    std::vector<MatrixVertex> m_vertices;
    QImage m_atlas;
    std::array<QImage,7> m_tintedAtlases;
    QSize m_cell;
    double m_fontHeight=0;
    quint64 m_revision=0;
    std::optional<double> m_epochMs;
    qint64 m_presentationNs=0;
    bool m_hasPresentation=false,m_shaderFailed=false,m_shaderInUse=false,m_plainInUse=false;
    QMetaObject::Connection m_errorConnection;
    bool (*m_supportCheck)(QQuickWindow *,int,bool)=nullptr;
};

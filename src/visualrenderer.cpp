// SPDX-License-Identifier: GPL-3.0-or-later
#include <cstring>
#include "visualrenderer.h"
#include "visualmaterial.h"
#include <QDateTime>
#include <QQuickWindow>
#include <QSGGeometryNode>
#include <QSGSimpleTextureNode>
#include <QImage>
#include <cmath>
#include <ctime>

namespace {
struct VisualNode : QSGNode { bool shader=false, plain=false; uint32_t visual=1; QRectF bounds; };
qint64 monotonicNs() {
    timespec value{};
    clock_gettime(CLOCK_MONOTONIC,&value);
    return value.tv_sec*1000000000LL+value.tv_nsec;
}
}
VisualRenderer::VisualRenderer(QQuickItem *parent) : QQuickItem(parent) {
    setFlag(ItemHasContents);
    const auto observe=[this](QQuickWindow *window) {
        disconnect(m_errorConnection); m_shaderFailed=false;
        if (window) m_errorConnection=connect(window,&QQuickWindow::sceneGraphError,this,
            [this](QQuickWindow::SceneGraphError,const QString &) {m_shaderFailed=true; update();});
    };
    connect(this,&QQuickItem::windowChanged,this,observe);
    observe(window());
}
void VisualRenderer::configureAurora(int density,qreal scale,qreal speed,qreal trails,
    const QString &palette,int seed,bool reduced,double epochMs) {
    configure(VISUALS_CORE_AURORA,density,scale,speed,trails,palette,seed,reduced,epochMs);
}
void VisualRenderer::configureConstellation(int density,qreal scale,qreal speed,qreal trails,
    const QString &palette,int seed,bool reduced,double epochMs) {
    configure(VISUALS_CORE_CONSTELLATION,density,scale,speed,trails,palette,seed,reduced,epochMs);
}
void VisualRenderer::configureStarfield(int density,qreal scale,qreal speed,qreal trails,
    const QString &palette,int seed,bool reduced,double epochMs) {
    configure(VISUALS_CORE_STARFIELD,density,scale,speed,trails,palette,seed,reduced,epochMs);
}
void VisualRenderer::configure(uint32_t visual,int density,qreal scale,qreal speed,qreal trails,
    const QString &palette,int seed,bool reduced,double epochMs) {
    if (!std::isfinite(epochMs) || std::abs(epochMs)>1e13) return;
    static const QStringList names={QStringLiteral("ocean"),QStringLiteral("spectrum"),QStringLiteral("ember"),
        QStringLiteral("forest"),QStringLiteral("mono"),QStringLiteral("pastel")};
    const auto index=names.indexOf(palette);
    // Convert the shared wall-clock epoch once. PresentationClock supplies
    // CLOCK_MONOTONIC timestamps; no Date.now()/property changes per frame.
    const double mappedEpoch=double(monotonicNs())+(epochMs-QDateTime::currentMSecsSinceEpoch())*1000000.0;
    if (mappedEpoch<=double(INT64_MIN) || mappedEpoch>=double(INT64_MAX)) return;
    const qint64 epochNs=m_epochMs && *m_epochMs==epochMs ? m_config.epoch_ns : qint64(mappedEpoch);
    visuals_core_config config{VISUALS_CORE_ABI_VERSION,sizeof(visuals_core_config),visual,
        uint32_t(index<0 ? 0 : index),double(density),scale,speed,trails,seed,uint32_t(reduced),epochNs};
    if (!m_core) m_core.reset(visuals_core_create(&config));
    else if (visuals_core_configure(m_core.get(),&config)!=0) return;
    if (!m_core) return;
    m_config=config; m_epochMs=epochMs;
    if (visual!=VISUALS_CORE_AURORA) m_primitives.resize(visual==VISUALS_CORE_CONSTELLATION ? VISUALS_CORE_MAX_PRIMITIVES : 356);
    captureSnapshot(false);
}
void VisualRenderer::setViewport(const QRectF &bounds,const QPointF &origin) {
    if (m_worldBounds==bounds && m_viewportOrigin==origin) return;
    m_worldBounds=bounds; m_viewportOrigin=origin;
    captureSnapshot(false);
}
void VisualRenderer::presentAt(qint64 ns) {
    if (ns<0) return;
    m_presentationNs=ns; m_hasPresentation=true;
    captureSnapshot(true);
}
void VisualRenderer::captureSnapshot(bool advance) {
    if (!m_core || width()<=0 || height()<=0) return;
    const visuals_core_request request{width(),height(),m_presentationNs,uint32_t(advance && m_hasPresentation),0,
        {m_worldBounds.x(),m_worldBounds.y(),m_worldBounds.width(),m_worldBounds.height()},
        {m_viewportOrigin.x(),m_viewportOrigin.y()}};
    const int result=m_config.visual==VISUALS_CORE_AURORA
        ? visuals_core_frame(m_core.get(),&request,&m_snapshot,sizeof(m_snapshot))
        : visuals_core_primitives(m_core.get(),&request,m_primitives.data(),m_primitives.size(),&m_primitiveCount);
    if (result==0) update();
}
void VisualRenderer::geometryChange(const QRectF &now,const QRectF &old) {
    QQuickItem::geometryChange(now,old);
    if (now.size()!=old.size()) captureSnapshot(false);
}
QSGNode *VisualRenderer::updatePaintNode(QSGNode *old,UpdatePaintNodeData *) {
    // GUI is blocked throughout sync; materials keep their own immutable copy.
    // Draw callbacks only see node-owned resources, never this item or Rust handle.
    if (old && static_cast<VisualNode *>(old)->visual!=m_config.visual) {delete old;old=nullptr;}
    if (m_config.visual!=VISUALS_CORE_AURORA) return updatePrimitives(old);
    m_plainInUse=false;
    if (!m_snapshot.band_count || width()<=0 || height()<=0) {delete old; return nullptr;}
    auto *root=static_cast<VisualNode *>(old);
    bool shader=root ? root->shader && !m_shaderFailed : false;
    if (!root && !m_shaderFailed) shader=m_supportCheck ? m_supportCheck(window()) : VisualMaterial::supportsWindow(window());
    m_shaderInUse=shader;
    if (root && root->shader!=shader) {delete root; root=nullptr;}
    if (!root) {
        root=new VisualNode; root->shader=shader;
        qInfo("Visuals Aurora: %s",shader ? "GPU fragment shader" : "Rust RGBA image fallback");
        if (shader) {
            auto *node=new QSGGeometryNode;
            auto *geometry=new QSGGeometry(QSGGeometry::defaultAttributes_TexturedPoint2D(),4);
            geometry->setDrawingMode(QSGGeometry::DrawTriangleStrip);
            node->setGeometry(geometry); node->setFlag(QSGNode::OwnsGeometry);
            node->setMaterial(new VisualMaterial); node->setFlag(QSGNode::OwnsMaterial);
            root->appendChildNode(node);
        } else {
            auto *node=new QSGSimpleTextureNode;
            node->setOwnsTexture(true);
            root->appendChildNode(node);
        }
    }
    if (shader) {
        auto *node=static_cast<QSGGeometryNode *>(root->firstChild());
        if (root->bounds!=boundingRect()) {
            root->bounds=boundingRect();
            QSGGeometry::updateTexturedRectGeometry(node->geometry(),root->bounds,QRectF(0,0,1,1));
            node->markDirty(QSGNode::DirtyGeometry);
        }
        static_cast<VisualMaterial *>(node->material())->snapshot=m_snapshot;
        node->markDirty(QSGNode::DirtyMaterial);
    } else if (window()) {
        const qreal dpr=window()->effectiveDevicePixelRatio();
        QImage image(QSize(qCeil(width()*dpr),qCeil(height()*dpr)),QImage::Format_RGBA8888_Premultiplied);
        image.fill(Qt::transparent); // Rust borrows the pixels as an initialised slice.
        if (!image.isNull() && visuals_core_raster(&m_snapshot,image.width(),image.height(),image.bits(),size_t(image.sizeInBytes()))==0) {
            auto *node=static_cast<QSGSimpleTextureNode *>(root->firstChild());
            auto *texture=window()->createTextureFromImage(image);
            if (texture) {
                node->setTexture(texture); // setOwnsTexture transfers replacement cleanup to the node.
                node->setRect(boundingRect());
            }
        }
    }
    return root;
}

QSGNode *VisualRenderer::updatePrimitives(QSGNode *old) {
    if (!m_primitiveCount || width()<=0 || height()<=0) {delete old;return nullptr;}
    auto *root=static_cast<VisualNode *>(old);
    bool shader=root ? root->shader && !m_shaderFailed : false;
    bool plain=root ? root->plain && !m_shaderFailed : false;
    if (!root && !m_shaderFailed) {
        shader=m_supportCheck ? m_supportCheck(window()) : VisualMaterial::supportsWindow(window(),VisualMaterial::PrimitiveTexture,int(m_primitives.size()));
        if (!shader) plain=VisualMaterial::supportsWindow(window(),VisualMaterial::PrimitivePlain);
    }
    m_shaderInUse=shader;m_plainInUse=plain;
    if (root && (root->shader!=shader || root->plain!=plain)) {delete root;root=nullptr;}
    if (!root) {
        root=new VisualNode;root->shader=shader;root->plain=plain;root->visual=m_config.visual;
        qInfo("Visuals %s: %s",m_config.visual==VISUALS_CORE_CONSTELLATION ? "Constellation" : "Starfield",
            shader ? "GPU RGBA32F primitive records" : plain ? "Rust plain geometry fallback" : "Rust RGBA image fallback (software/no primitive pipeline)");
        if (shader || plain) {
            auto *node=new QSGGeometryNode;
            QSGGeometry *geometry=nullptr;
            if (shader) {
                geometry=new QSGGeometry(QSGGeometry::defaultAttributes_TexturedPoint2D(),int(m_primitives.size())*6);
                geometry->setVertexDataPattern(QSGGeometry::StaticPattern);
            } else {
                static const QSGGeometry::Attribute attrs[]={
                    QSGGeometry::Attribute::create(0,2,QSGGeometry::FloatType,true),
                    QSGGeometry::Attribute::create(1,2,QSGGeometry::FloatType),
                    QSGGeometry::Attribute::create(2,4,QSGGeometry::FloatType),
                    QSGGeometry::Attribute::create(3,4,QSGGeometry::FloatType)};
                static const QSGGeometry::AttributeSet attributes={4,sizeof(visuals_core_vertex),attrs};
                geometry=new QSGGeometry(attributes,int(m_primitiveCount)*6);
                std::memset(geometry->vertexData(),0,size_t(geometry->vertexCount())*geometry->sizeOfVertex());
            }
            geometry->setDrawingMode(QSGGeometry::DrawTriangles);
            node->setGeometry(geometry);node->setFlag(QSGNode::OwnsGeometry);
            node->setMaterial(new VisualMaterial(shader ? VisualMaterial::PrimitiveTexture : VisualMaterial::PrimitivePlain));
            node->setFlag(QSGNode::OwnsMaterial);root->appendChildNode(node);
        } else {
            auto *node=new QSGSimpleTextureNode;node->setOwnsTexture(true);root->appendChildNode(node);
        }
    }
    const qreal dpr=window() ? window()->effectiveDevicePixelRatio() : 1;
    if (shader || plain) {
        auto *node=static_cast<QSGGeometryNode *>(root->firstChild());
        auto *material=static_cast<VisualMaterial *>(node->material());
        material->primitiveCount=m_primitiveCount;material->recordCapacity=int(m_primitives.size());material->fringe=1.0f/dpr;material->primitiveViewport=size();
        if (shader) {
            // Positions bound the whole item for scene-graph culling. Only
            // viewport resizing rewrites this retained repeated-quad geometry.
            if (root->bounds!=boundingRect()) {
                root->bounds=boundingRect();
                const float corners[6][2]={{0,0},{1,0},{0,1},{0,1},{1,0},{1,1}};
                auto *v=node->geometry()->vertexDataAsTexturedPoint2D();
                for (size_t i=0;i<m_primitives.size();++i) for(int k=0;k<6;++k)
                    v[i*6+k].set(corners[k][0]*width(),corners[k][1]*height(),float(i),0);
                node->markDirty(QSGNode::DirtyGeometry);
            }
            material->dataTexture->setData(QByteArray(reinterpret_cast<const char *>(m_primitives.data()),m_primitiveCount*sizeof(visuals_core_primitive)),
                {3,int(m_primitives.size())},{3,int(m_primitiveCount)});
        } else {
            // Rust borrows the buffer as an initialised slice, so zero new storage.
            if (node->geometry()->vertexCount()!=int(m_primitiveCount)*6) {
                node->geometry()->allocate(int(m_primitiveCount)*6);
                std::memset(node->geometry()->vertexData(),0,size_t(node->geometry()->vertexCount())*node->geometry()->sizeOfVertex());
            }
            visuals_core_primitive_vertices(m_primitives.data(),m_primitiveCount,material->fringe,
                static_cast<visuals_core_vertex *>(node->geometry()->vertexData()),size_t(node->geometry()->vertexCount()));
            node->markDirty(QSGNode::DirtyGeometry);
        }
        node->markDirty(QSGNode::DirtyMaterial);
    } else if (window()) {
        QImage image(QSize(qCeil(width()*dpr),qCeil(height()*dpr)),QImage::Format_RGBA8888_Premultiplied);
        image.fill(Qt::transparent); // Rust borrows the pixels as an initialised slice.
        if (!image.isNull() && visuals_core_primitive_raster(m_primitives.data(),m_primitiveCount,width(),height(),
            image.width(),image.height(),image.bits(),size_t(image.sizeInBytes()))==0) {
            auto *node=static_cast<QSGSimpleTextureNode *>(root->firstChild());
            if (auto *texture=window()->createTextureFromImage(image)) {node->setTexture(texture);node->setRect(boundingRect());}
        }
    }
    return root;
}

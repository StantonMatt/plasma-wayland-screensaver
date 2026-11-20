// SPDX-License-Identifier: GPL-3.0-or-later
#include "strokerenderer.h"
#include "strokematerial.h"
#include <QDateTime>
#include <QQuickWindow>
#include <QSGGeometryNode>
#include <QSGSimpleTextureNode>
#include <QSGRenderNode>
#include <rhi/qrhi.h>
#include <array>
#include <QImage>
#include <cmath>
#include <cstring>
#include <ctime>
namespace {
struct StrokeNode : QSGNode {bool shader=false;};
qint64 monotonicNs() {timespec t{};clock_gettime(CLOCK_MONOTONIC,&t);return t.tv_sec*1000000000LL+t.tv_nsec;}
bool strokeNeedsUnion(const visuals_strokes_vertex *v,size_t count) {
    // Writer::stroke is a connected quad strip. If both edges advance on a
    // common axis and every triangle has the same nonzero orientation, its
    // piecewise affine parameterization is one-to-one: x increases along t,
    // and at fixed x, y changes monotonically across s (det J / dx/dt).
    // This proves disjoint interiors, including nonadjacent segments. Anything
    // that cannot satisfy this sufficient condition uses the coverage union.
    const double axis[]={double(v[count-4].position[0])+v[count-1].position[0]-v[0].position[0]-v[1].position[0],
        double(v[count-4].position[1])+v[count-1].position[1]-v[0].position[1]-v[1].position[1]};
    double orientation=0;
    for (size_t i=0;i<count;i+=6) {
        for (int side=0;side<2;++side) {
            const auto &a=v[i+side],&b=v[i+(side?5:2)];
            if ((double(b.position[0])-a.position[0])*axis[0]+(double(b.position[1])-a.position[1])*axis[1]<=0) return true;
        }
        for (size_t t=i;t<i+6;t+=3) {
            const auto &a=v[t],&b=v[t+1],&c=v[t+2];
            const double area=(double(b.position[0])-a.position[0])*(double(c.position[1])-a.position[1])
                -(double(b.position[1])-a.position[1])*(double(c.position[0])-a.position[0]);
            if (std::abs(area)<1e-6 || (orientation!=0 && (area>0)!=(orientation>0))) return true;
            orientation=area;
        }
    }
    return false;
}
// One private pass for the complete visual. Each stroke first selects maximum
// coverage in depth, then adds that coverage once using stencil. Depth bands
// order strokes without clearing between them; no per-stroke targets or passes.
class StrokeUnionNode final : public QSGRenderNode {
public:
    explicit StrokeUnionNode(QQuickWindow *window):m_window(window) {}
    ~StrokeUnionNode() override {releaseResources();}
    bool initialize(const QSize &size,int vertexCount) {
        auto *rhi=static_cast<QRhi *>(m_window->rendererInterface()->getResource(m_window,QSGRendererInterface::RhiResource));
        if (!rhi) return false;
        if (m_rhi!=rhi || m_size!=size || m_capacity!=vertexCount) {
            releaseResources();m_rhi=rhi;m_size=size;m_capacity=vertexCount;
            m_texture.reset(rhi->newTexture(QRhiTexture::RGBA8,size,1,QRhiTexture::RenderTarget));
            m_depth.reset(rhi->newRenderBuffer(QRhiRenderBuffer::DepthStencil,size,1));
            if (!m_texture->create() || !m_depth->create()) return false;
            QRhiTextureRenderTargetDescription description{QRhiColorAttachment(m_texture.get())};
            description.setDepthStencilBuffer(m_depth.get());
            m_target.reset(rhi->newTextureRenderTarget(description));
            m_pass.reset(m_target->newCompatibleRenderPassDescriptor());m_target->setRenderPassDescriptor(m_pass.get());
            if (!m_target->create()) return false;
            m_vertices.reset(rhi->newBuffer(QRhiBuffer::Dynamic,QRhiBuffer::VertexBuffer,vertexCount*sizeof(visuals_strokes_vertex)));
            m_stride=rhi->ubufAligned(StrokeMaterial::UniformSize);
            m_uniforms.reset(rhi->newBuffer(QRhiBuffer::Dynamic,QRhiBuffer::UniformBuffer,m_stride*MaxPrimitives));
            if (!m_vertices->create() || !m_uniforms->create()) return false;
            m_bindings.reset(rhi->newShaderResourceBindings());
            m_bindings->setBindings({QRhiShaderResourceBinding::uniformBufferWithDynamicOffset(0,
                QRhiShaderResourceBinding::VertexStage|QRhiShaderResourceBinding::FragmentStage,
                m_uniforms.get(),StrokeMaterial::UniformSize)});
            if (!m_bindings->create()) return false;
            QRhiVertexInputLayout layout;layout.setBindings({{sizeof(visuals_strokes_vertex)}});
            layout.setAttributes({{0,0,QRhiVertexInputAttribute::Float2,0},{0,1,QRhiVertexInputAttribute::Float4,8},
                {0,2,QRhiVertexInputAttribute::Float2,24},{0,3,QRhiVertexInputAttribute::Float4,32}});
            for (int i=0;i<3;++i) {
                auto &pipeline=m_pipeline[i];pipeline.reset(rhi->newGraphicsPipeline());
                pipeline->setShaderStages({{QRhiShaderStage::Vertex,StrokeMaterial::vertexShader()},
                    {QRhiShaderStage::Fragment,StrokeMaterial::fragmentShader()}});
                pipeline->setVertexInputLayout(layout);pipeline->setShaderResourceBindings(m_bindings.get());
                pipeline->setRenderPassDescriptor(m_pass.get());pipeline->setDepthTest(i<2);
                pipeline->setDepthWrite(i==0);pipeline->setDepthOp(i==0?QRhiGraphicsPipeline::Less:QRhiGraphicsPipeline::Equal);
                pipeline->setStencilTest(i<2);pipeline->setFlags(QRhiGraphicsPipeline::UsesStencilRef);
                QRhiGraphicsPipeline::StencilOpState stencil;
                // Prepass clears ownership wherever the new stroke wins depth.
                // Colour pass consumes that pixel exactly once, including ties.
                stencil.compareOp=i==0?QRhiGraphicsPipeline::Always:QRhiGraphicsPipeline::Equal;
                stencil.passOp=i==0?QRhiGraphicsPipeline::StencilZero:QRhiGraphicsPipeline::IncrementAndClamp;
                pipeline->setStencilFront(stencil);pipeline->setStencilBack(stencil);
                QRhiGraphicsPipeline::TargetBlend blend;
                if (i==0) blend.colorWrite={};
                else {blend.enable=true;blend.srcColor=blend.dstColor=blend.srcAlpha=blend.dstAlpha=QRhiGraphicsPipeline::One;}
                pipeline->setTargetBlends({blend});if (!pipeline->create()) return false;
            }
            m_data.resize(m_stride*MaxPrimitives);m_data.fill(0);m_snapshot.resize(vertexCount);
            auto *texture=m_window->createTextureFromRhiTexture(m_texture.get(),QQuickWindow::TextureHasAlphaChannel);
            if (!texture) return false;
            // The public wrapper takes ownership of QRhiTexture (Qt 6.6+).
            // Keep the unique_ptr only until that transfer succeeds.
            (void)m_texture.release();
            m_display=new QSGSimpleTextureNode;m_display->setTexture(texture);
            m_display->setOwnsTexture(true);appendChildNode(m_display);
        }
        return true;
    }
    bool snapshot(const std::vector<visuals_strokes_vertex> &vertices,const QSizeF &logical,qreal dpr) {
        m_logical=logical;m_dpr=dpr;m_count=0;
        // Circle pairs are one six-vertex primitive. A strip begins with its
        // negative butt fringe; all following quad starts have positive cap
        // distance. This preserves the frozen 48-byte vertex ABI.
        for (size_t start=0;start<vertices.size();) {
            size_t end=start+6;
            if (vertices[start].parameters[1]<0) while (end<vertices.size() && vertices[end].parameters[1]<0 && vertices[end].shape[1]>=0) end+=6;
            if (m_count==MaxPrimitives || end>vertices.size()) return false;
            const bool needsUnion=vertices[start].parameters[1]<0 && strokeNeedsUnion(vertices.data()+start,end-start);
            m_ranges[m_count++]={int(start),int(end-start),needsUnion};start=end;
        }
        std::memcpy(m_snapshot.data(),vertices.data(),vertices.size()*sizeof(visuals_strokes_vertex));
        m_display->setRect(QRectF(QPointF(0,0),logical));
        markDirty(QSGNode::DirtyForceUpdate);return true;
    }
    void prepare() override {
        auto *cb=commandBuffer();if (!cb || !m_target) return;
        QMatrix4x4 projection;
        if (m_rhi->isYUpInFramebuffer()) projection.scale(1,-1);
        projection.ortho(0,float(m_logical.width()),float(m_logical.height()),0,-1,1);
        projection=m_rhi->clipSpaceCorrMatrix()*projection;
        for (int i=0;i<m_count;++i) {
            auto *data=m_data.data()+i*m_stride;std::memcpy(data,projection.constData(),64);
            const float info[]={1,float(m_dpr),float(i),1};std::memcpy(data+64,info,sizeof(info));
        }
        auto *updates=m_rhi->nextResourceUpdateBatch();
        updates->updateDynamicBuffer(m_vertices.get(),0,m_snapshot.size()*sizeof(visuals_strokes_vertex),m_snapshot.data());
        updates->updateDynamicBuffer(m_uniforms.get(),0,m_count*m_stride,m_data.constData());
        cb->beginPass(m_target.get(),Qt::transparent,{1,0},updates);
        const QRhiCommandBuffer::VertexInput input(m_vertices.get(),0);
        // Circle pairs and proven nonoverlapping strips can share one ordinary
        // additive draw. Only folded/uncertain strips need depth/stencil work.
        for (int i=0;i<m_count;) {
            int end=i+1,count=m_ranges[i].count;
            if (!m_ranges[i].needsUnion) while (end<m_count && !m_ranges[end].needsUnion) count+=m_ranges[end++].count;
            for (int pass=m_ranges[i].needsUnion?0:2;pass<(m_ranges[i].needsUnion?2:3);++pass) {
                cb->setGraphicsPipeline(m_pipeline[pass].get());
                cb->setViewport({0,0,float(m_size.width()),float(m_size.height())});cb->setStencilRef(0);
                const QRhiCommandBuffer::DynamicOffset offset{0,quint32(i*m_stride)};
                cb->setShaderResources(m_bindings.get(),1,&offset);cb->setVertexInput(0,1,&input);
                cb->draw(count,1,m_ranges[i].first);
            }
            i=end;
        }
        cb->endPass();
    }
    void render(const RenderState *) override {}
    RenderingFlags flags() const override {return NoExternalRendering;}
    void releaseResources() override {
        for (auto &p:m_pipeline) p.reset();
        m_bindings.reset();m_uniforms.reset();m_vertices.reset();
        m_target.reset();m_pass.reset();m_depth.reset();
        delete m_display;m_display=nullptr; // Deletes its owned QRhiTexture last.
        m_texture.reset();m_rhi=nullptr;m_size={};m_capacity=0;
    }
private:
    static constexpr int MaxPrimitives=512;
    struct Range {int first,count;bool needsUnion;};
    QQuickWindow *m_window;QRhi *m_rhi=nullptr;QSize m_size;QSizeF m_logical;qreal m_dpr=1;
    int m_capacity=0,m_stride=0,m_count=0;std::array<Range,MaxPrimitives> m_ranges{};
    std::vector<visuals_strokes_vertex> m_snapshot;QByteArray m_data;
    std::unique_ptr<QRhiTexture> m_texture;std::unique_ptr<QRhiRenderBuffer> m_depth;
    std::unique_ptr<QRhiTextureRenderTarget> m_target;std::unique_ptr<QRhiRenderPassDescriptor> m_pass;
    std::unique_ptr<QRhiBuffer> m_vertices,m_uniforms;std::unique_ptr<QRhiShaderResourceBindings> m_bindings;
    std::unique_ptr<QRhiGraphicsPipeline> m_pipeline[3];QSGSimpleTextureNode *m_display=nullptr;
};

}
StrokeRenderer::StrokeRenderer(QQuickItem *parent):QQuickItem(parent) {
    setFlag(ItemHasContents);
    const auto observe=[this](QQuickWindow *window) {
        disconnect(m_errorConnection);m_shaderFailed=false;
        if (window) m_errorConnection=connect(window,&QQuickWindow::sceneGraphError,this,
            [this](QQuickWindow::SceneGraphError,const QString &) {m_shaderFailed=true;update();});
    };
    connect(this,&QQuickItem::windowChanged,this,observe);observe(window());
}
void StrokeRenderer::configureStrokes(const QString &visual,int density,qreal scale,qreal speed,qreal trails,
    const QString &palette,int seed,bool reduced,double epochMs) {
    const uint32_t kind=visual==QStringLiteral("ribbons")?VISUALS_CORE_RIBBONS:
        visual==QStringLiteral("kaleidoscope")?VISUALS_CORE_KALEIDOSCOPE:0;
    if (!kind || !std::isfinite(epochMs) || std::abs(epochMs)>1e13) return;
    static const QStringList names={QStringLiteral("ocean"),QStringLiteral("spectrum"),QStringLiteral("ember"),
        QStringLiteral("forest"),QStringLiteral("mono"),QStringLiteral("pastel")};
    const auto index=names.indexOf(palette);
    const double mapped=double(monotonicNs())+(epochMs-QDateTime::currentMSecsSinceEpoch())*1000000.0;
    if (mapped<=double(INT64_MIN) || mapped>=double(INT64_MAX)) return;
    const auto epoch=m_epochMs && *m_epochMs==epochMs?m_config.epoch_ns:qint64(mapped);
    visuals_core_config c{VISUALS_CORE_ABI_VERSION,sizeof(visuals_core_config),kind,uint32_t(index<0?0:index),
        double(density),scale,speed,trails,seed,uint32_t(reduced),epoch};
    if (!m_core) m_core.reset(visuals_strokes_create(&c));
    else if (visuals_strokes_configure(m_core.get(),&c)!=0) return;
    if (!m_core) return;
    m_config=c;m_epochMs=epochMs;captureSnapshot(false);
}
void StrokeRenderer::setViewport(const QRectF &bounds,const QPointF &origin) {
    if (bounds==m_worldBounds && origin==m_viewportOrigin) return;
    m_worldBounds=bounds;m_viewportOrigin=origin;captureSnapshot(false);
}
void StrokeRenderer::presentAt(qint64 ns) {
    if (ns<0) return;
    m_presentationNs=ns;m_hasPresentation=true;captureSnapshot(true);
}
void StrokeRenderer::captureSnapshot(bool advance) {
    if (!m_core || width()<=0 || height()<=0) return;
    const visuals_core_request r{width(),height(),m_presentationNs,uint32_t(advance && m_hasPresentation),0,
        {m_worldBounds.x(),m_worldBounds.y(),m_worldBounds.width(),m_worldBounds.height()},
        {m_viewportOrigin.x(),m_viewportOrigin.y()}};
    const auto count=visuals_strokes_vertex_count(m_core.get(),&r);
    if (!count) return;
    // Topology is phase-independent: resize only allocates on viewport/settings changes.
    m_vertices.resize(count);
    if (visuals_strokes_frame(m_core.get(),&r,m_vertices.data(),m_vertices.size())==int(count)) update();
}
void StrokeRenderer::geometryChange(const QRectF &now,const QRectF &old) {
    QQuickItem::geometryChange(now,old);if (now.size()!=old.size()) captureSnapshot(false);
}
QSGNode *StrokeRenderer::updatePaintNode(QSGNode *old,UpdatePaintNodeData *) {
    if (m_vertices.empty() || width()<=0 || height()<=0) {delete old;return nullptr;}
    auto *root=static_cast<StrokeNode *>(old);
    bool shader=root?root->shader && !m_shaderFailed:false;
    if (!root && !m_shaderFailed) shader=m_supportCheck?m_supportCheck(window()):StrokeMaterial::supportsWindow(window());
    m_shaderInUse=shader;
    if (root && root->shader!=shader) {delete root;root=nullptr;}
    if (!root) {
        root=new StrokeNode;root->shader=shader;
        qInfo("Visuals %s: %s",m_config.visual==VISUALS_CORE_RIBBONS?"Ribbons":"Kaleidoscope",
            shader?"GPU stroke union / additive RGBA8 layer":"Rust RGBA image fallback");
        if (shader) root->appendChildNode(new StrokeUnionNode(window()));
        else {
            auto *node=new QSGSimpleTextureNode;node->setOwnsTexture(true);root->appendChildNode(node);
        }
    }
    if (shader) {
        const auto dpr=window()->effectiveDevicePixelRatio();
        auto *node=static_cast<StrokeUnionNode *>(root->firstChild());
        if (!node->initialize(QSize(qCeil(width()*dpr),qCeil(height()*dpr)),int(m_vertices.size())) ||
            !node->snapshot(m_vertices,size(),dpr)) {
            // Resource/pipeline creation is fallible on every RHI backend.
            delete root;m_shaderFailed=true;m_shaderInUse=false;return updatePaintNode(nullptr,nullptr);
        }
    } else if (window()) {
        const auto dpr=window()->effectiveDevicePixelRatio();
        QImage image(QSize(qCeil(width()*dpr),qCeil(height()*dpr)),QImage::Format_RGBA8888_Premultiplied);
        image.fill(Qt::transparent); // Rust borrows the pixels as an initialised slice.
        if (!image.isNull() && visuals_strokes_raster(m_vertices.data(),m_vertices.size(),width(),height(),
            image.width(),image.height(),image.bits(),size_t(image.sizeInBytes()))==0) {
            auto *texture=window()->createTextureFromImage(image);
            if (texture) {auto *node=static_cast<QSGSimpleTextureNode *>(root->firstChild());node->setTexture(texture);node->setRect(boundingRect());}
        }
    }
    return root;
}

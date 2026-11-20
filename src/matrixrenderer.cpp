// SPDX-License-Identifier: GPL-3.0-or-later
#include "matrixrenderer.h"
#include <QQuickWindow>
#include <QSGGeometryNode>
#include <QSGSimpleTextureNode>
#include <QDateTime>
#include <QFontMetricsF>
#include <QTextLayout>
#include <QPainter>
#include <QGlyphRun>
#include <cmath>
#include <ctime>
#include <cstring>
namespace {
constexpr int AtlasColumns=8,AtlasScale=4,Padding=4;
const QString Glyphs=QStringLiteral("0123456789ABCDEF<>[]{}アイウエオカキクケコサシスセソ");
struct Node:QSGNode {bool shader=false,plain=false;quint64 revision=0;int columns=0;QSize atlasSize;};
qint64 monotonicNs(){timespec t{};clock_gettime(CLOCK_MONOTONIC,&t);return t.tv_sec*1000000000LL+t.tv_nsec;}
QFont matrixFont(int size){QFont f(QStringLiteral("monospace"));f.setPixelSize(size);return f;}
}
MatrixRenderer::MatrixRenderer(QQuickItem *parent):QQuickItem(parent) {
    setFlag(ItemHasContents);setClip(true);
    const auto observe=[this](QQuickWindow *w){disconnect(m_errorConnection);m_shaderFailed=false;
        if(w)m_errorConnection=connect(w,&QQuickWindow::sceneGraphError,this,[this](QQuickWindow::SceneGraphError,const QString &){m_shaderFailed=true;update();});};
    connect(this,&QQuickItem::windowChanged,this,observe);observe(window());
}
void MatrixRenderer::configureMatrix(int density,qreal scale,qreal speed,qreal trails,const QString &palette,int seed,bool reduced,double epochMs){
    if(!std::isfinite(epochMs)||std::abs(epochMs)>1e13)return;
    static const QStringList names={QStringLiteral("ocean"),QStringLiteral("spectrum"),QStringLiteral("ember"),QStringLiteral("forest"),QStringLiteral("mono"),QStringLiteral("pastel")};
    const int p=names.indexOf(palette);
    const double mapped=double(monotonicNs())+(epochMs-QDateTime::currentMSecsSinceEpoch())*1e6;
    if(mapped<=double(INT64_MIN)||mapped>=double(INT64_MAX))return;
    const qint64 epoch=m_epochMs&&*m_epochMs==epochMs?m_config.epoch_ns:qint64(mapped);
    visuals_core_config config{VISUALS_CORE_ABI_VERSION,sizeof(visuals_core_config),VISUALS_MATRIX_VISUAL,uint32_t(p<0?0:p),double(density),scale,speed,trails,seed,uint32_t(reduced),epoch};
    const bool layoutChanged=!m_core||density!=m_config.density||scale!=m_config.scale||trails!=m_config.trails||config.palette!=m_config.palette||seed!=m_config.seed;
    if(!m_core)m_core.reset(visuals_matrix_create(&config));
    else if(visuals_matrix_configure(m_core.get(),&config)!=0)return;
    if(!m_core)return;
    m_config=config;m_epochMs=epochMs;
    if(layoutChanged)rebuildLayout();
    capture(false);
}
void MatrixRenderer::rebuildLayout(){
    if(!m_core||width()<=0||height()<=0)return;
    const auto oldSize=m_layout.glyph_size;
    if(visuals_matrix_get_layout(m_core.get(),width(),&m_layout)!=0)return;
    m_columns.resize(m_layout.column_count);m_offsets.resize(m_layout.column_count*4);
    m_fontHeight=std::ceil(QFontMetricsF(matrixFont(int(m_layout.glyph_size))).height());
    if(visuals_matrix_columns(m_core.get(),width(),m_fontHeight,m_columns.data(),m_columns.size())!=0)return;
    if(oldSize!=m_layout.glyph_size)m_glyphs.clear();
    m_quads.clear();
    const size_t oldGlyphs=m_glyphs.size();
    // Shape the complete multiline stream, like QQuickText. Common-script
    // digits/punctuation can inherit a Japanese run's fallback font across a
    // line separator, so shaping isolated Unicode characters changes the face.
    const auto shape=[&](const QString &string,uint32_t column,double top,const float *color,float opacity,bool stream){
        QTextLayout text(string,matrixFont(int(m_layout.glyph_size)));
        QTextOption option;option.setUseDesignMetrics(true);option.setAlignment(Qt::AlignHCenter);option.setWrapMode(QTextOption::NoWrap);text.setTextOption(option);
        text.beginLayout();int row=0;
        for(;;){auto line=text.createLine();if(!line.isValid())break;line.setLineWidth(m_layout.column_width);line.setPosition({0,row++*(stream?m_layout.line_height:0)});}
        text.endLayout();
        for(int row=0;row<text.lineCount();++row)for(const auto &run:text.lineAt(row).glyphRuns()){
            const auto ids=run.glyphIndexes();const auto positions=run.positions();
            for(qsizetype i=0;i<ids.size();++i){
                size_t glyph=0;for(;glyph<m_glyphs.size();++glyph)if(m_glyphs[glyph].index==ids[i]&&m_glyphs[glyph].font==run.rawFont())break;
                if(glyph==m_glyphs.size())m_glyphs.push_back({run.rawFont(),ids[i]});
                m_quads.push_back({m_columns[column].x+positions[i].x(),top+positions[i].y(),glyph,column,{color[0]*opacity,color[1]*opacity,color[2]*opacity,opacity}});
            }
        }
    };
    for(uint32_t i=0;i<m_columns.size();++i){const auto &col=m_columns[i];QString stream;
        for(uint32_t row=0;row<col.length;++row){if(row)stream+=QChar::LineSeparator;stream+=Glyphs[col.glyphs[row]];}
        shape(stream,i,0,col.color,m_layout.stream_opacity,true);
        const float white[]={1,1,1,1};shape(QString(Glyphs[col.glyphs[col.length-1]]),i,col.head_y,white,m_layout.head_opacity,false);
    }
    if(m_atlas.isNull()||oldSize!=m_layout.glyph_size||oldGlyphs!=m_glyphs.size())buildAtlas();
    buildVertices();++m_revision;
}
void MatrixRenderer::buildAtlas(){
    // The atlas stores unique (resolved QRawFont, glyph index) pairs, including
    // context-dependent fallback variants of Latin digits and punctuation.
    double ascent=0,descent=0,right=0;
    for(const auto &glyph:m_glyphs){ascent=std::max(ascent,glyph.font.ascent());descent=std::max(descent,glyph.font.descent());right=std::max(right,glyph.font.boundingRect(glyph.index).right());}
    m_atlasBaseline=std::ceil(ascent)+Padding;
    m_cell={int(std::ceil(std::max(m_layout.column_width,right)))+Padding*2,int(std::ceil(ascent+descent))+Padding*2};
    m_atlasRows=int((m_glyphs.size()+AtlasColumns-1)/AtlasColumns);
    m_atlas=QImage(QSize(m_cell.width()*AtlasColumns,m_cell.height()*m_atlasRows)*AtlasScale,QImage::Format_Alpha8);m_atlas.fill(Qt::transparent);
    QPainter painter(&m_atlas);painter.scale(AtlasScale,AtlasScale);painter.setRenderHint(QPainter::TextAntialiasing);painter.setPen(Qt::white);
    for(size_t i=0;i<m_glyphs.size();++i){const auto &glyph=m_glyphs[i];QGlyphRun run;run.setRawFont(glyph.font);run.setGlyphIndexes({glyph.index});run.setPositions({{0,0}});
        painter.drawGlyphRun({qreal((i%AtlasColumns)*m_cell.width()+Padding),qreal((i/AtlasColumns)*m_cell.height()+m_atlasBaseline)},run);
    }
}
void MatrixRenderer::buildVertices(){
    m_vertices.resize(m_quads.size()*6);
    const float corners[6][2]={{0,0},{1,0},{0,1},{0,1},{1,0},{1,1}};
    size_t vertex=0;
    for(const auto &q:m_quads){
        const double x=q.x-Padding,top=q.y-m_atlasBaseline;
        const double u=(q.glyph%AtlasColumns)*m_cell.width(),v=(q.glyph/AtlasColumns)*m_cell.height();
        for(const auto &corner:corners){auto &out=m_vertices[vertex++];
            out={{float(corner[0]*width()),float(corner[1]*height())},
                {float(x+corner[0]*m_cell.width()),float(top+corner[1]*m_cell.height()),
                 float((u+corner[0]*m_cell.width())/(m_cell.width()*AtlasColumns)),float((v+corner[1]*m_cell.height())/(m_cell.height()*m_atlasRows))},
                {q.color[0],q.color[1],q.color[2],q.color[3]},float(q.column),0};
        }
    }
}
void MatrixRenderer::presentAt(qint64 ns){if(ns<0)return;m_presentationNs=ns;m_hasPresentation=true;capture(true);}
void MatrixRenderer::capture(bool advance){
    if(!m_core||m_columns.empty()||width()<=0||height()<=0)return;
    const visuals_core_request request{width(),height(),m_presentationNs,uint32_t(advance&&m_hasPresentation),0,{0,0,width(),height()},{0,0}};
    if(visuals_matrix_frame(m_core.get(),&request,m_offsets.data(),m_columns.size())==0)update();
}
void MatrixRenderer::geometryChange(const QRectF &now,const QRectF &old){QQuickItem::geometryChange(now,old);if(now.size()!=old.size()){rebuildLayout();capture(false);}}
QSGNode *MatrixRenderer::updatePaintNode(QSGNode *old,UpdatePaintNodeData *){
    if(m_vertices.empty()||width()<=0||height()<=0){delete old;return nullptr;}
    auto *root=static_cast<Node *>(old);
    // Re-probe the actual resource dimensions after a layout change. A window
    // can move from a small texture to one exceeding a device's size limit.
    if(root&&(root->columns!=int(m_columns.size())||root->atlasSize!=m_atlas.size())){delete root;root=nullptr;}
    const auto supported=[&](bool plain){return m_supportCheck?m_supportCheck(window(),int(m_columns.size()),plain):MatrixMaterial::supportsWindow(window(),int(m_columns.size()),plain,m_atlas.size());};
    bool shader=root?root->shader&&!m_shaderFailed:!m_shaderFailed&&supported(false);
    bool plain=root?root->plain&&!m_shaderFailed:!shader&&!m_shaderFailed&&supported(true);
    m_shaderInUse=shader;m_plainInUse=plain;
    if(root&&(root->shader!=shader||root->plain!=plain)){delete root;root=nullptr;}
    if(!root){root=new Node;root->shader=shader;root->plain=plain;root->columns=int(m_columns.size());root->atlasSize=m_atlas.size();
        qInfo("Visuals Matrix: %s",shader?"GPU glyph atlas + RGBA32F column offsets":plain?"plain glyph geometry fallback":"Qt glyph atlas image fallback (software/no glyph pipeline)");
        if(shader||plain){
            static const QSGGeometry::Attribute attrs[]={QSGGeometry::Attribute::create(0,2,QSGGeometry::FloatType,true),QSGGeometry::Attribute::create(1,4,QSGGeometry::FloatType),QSGGeometry::Attribute::create(2,4,QSGGeometry::FloatType),QSGGeometry::Attribute::create(3,1,QSGGeometry::FloatType)};
            static const QSGGeometry::AttributeSet attributes={4,sizeof(MatrixVertex),attrs};
            auto *node=new QSGGeometryNode;auto *geometry=new QSGGeometry(attributes,0);geometry->setDrawingMode(QSGGeometry::DrawTriangles);geometry->setVertexDataPattern(shader?QSGGeometry::StaticPattern:QSGGeometry::DynamicPattern);
            node->setGeometry(geometry);node->setFlag(QSGNode::OwnsGeometry);node->setMaterial(new MatrixMaterial(plain));node->setFlag(QSGNode::OwnsMaterial);root->appendChildNode(node);
        }else{auto *node=new QSGSimpleTextureNode;node->setOwnsTexture(true);root->appendChildNode(node);}
    }
    if(shader||plain){
        if(!m_tintedAtlases.front().isNull())m_tintedAtlases={};
        auto *node=static_cast<QSGGeometryNode *>(root->firstChild());auto *material=static_cast<MatrixMaterial *>(node->material());
        if(root->revision!=m_revision){
            if(node->geometry()->vertexCount()!=int(m_vertices.size()))node->geometry()->allocate(int(m_vertices.size()));
            std::memcpy(node->geometry()->vertexData(),m_vertices.data(),m_vertices.size()*sizeof(MatrixVertex));
            delete material->atlas;material->atlas=new MatrixAtlasTexture(m_atlas);
            if(material->atlas){material->atlas->setFiltering(QSGTexture::Linear);material->atlas->setMipmapFiltering(QSGTexture::Linear);}
            node->markDirty(QSGNode::DirtyGeometry);root->revision=m_revision;
        }
        material->columns=int(m_columns.size());
        if(shader)material->offsets.setData(QByteArray(reinterpret_cast<const char *>(m_offsets.data()),qsizetype(m_offsets.size()*sizeof(float))),{int(m_columns.size()),1});
        else{auto *vertices=static_cast<MatrixVertex *>(node->geometry()->vertexData());
            for(size_t i=0;i<m_vertices.size();++i){const auto &src=m_vertices[i];vertices[i].position[0]=src.local[0];vertices[i].position[1]=src.local[1]+m_offsets[size_t(src.column)*4];}node->markDirty(QSGNode::DirtyGeometry);}
        node->markDirty(QSGNode::DirtyMaterial);
    }else if(window()){
        if(root->revision!=m_revision){
    for(size_t i=0;i<m_tintedAtlases.size();++i){
        m_tintedAtlases[i]=m_atlas.convertToFormat(QImage::Format_RGBA8888_Premultiplied);QPainter tint(&m_tintedAtlases[i]);tint.setCompositionMode(QPainter::CompositionMode_SourceIn);
        if(i==6)tint.fillRect(m_atlas.rect(),QColor::fromRgbF(1,1,1,m_layout.head_opacity));
        else {const auto &c=m_columns[i%m_columns.size()].color;tint.fillRect(m_atlas.rect(),QColor::fromRgbF(c[0],c[1],c[2],m_layout.stream_opacity));}
    }
            root->revision=m_revision;
        }
        const qreal dpr=window()->effectiveDevicePixelRatio();QImage image(QSize(qCeil(width()*dpr),qCeil(height()*dpr)),QImage::Format_RGBA8888_Premultiplied);image.fill(Qt::transparent);
        QPainter painter(&image);painter.scale(dpr,dpr);painter.setRenderHint(QPainter::SmoothPixmapTransform);
        for(size_t i=0;i<m_vertices.size();i+=6){const auto &v=m_vertices[i];const bool head=v.color[0]==v.color[3]&&v.color[1]==v.color[3]&&v.color[2]==v.color[3]&&v.color[3]==m_layout.head_opacity;
            const QImage &colored=m_tintedAtlases[head?6:size_t(v.column)%6];
            painter.drawImage(QRectF(v.local[0],v.local[1]+m_offsets[size_t(v.column)*4],m_cell.width(),m_cell.height()),colored,QRectF(v.local[2]*m_atlas.width(),v.local[3]*m_atlas.height(),m_cell.width()*AtlasScale,m_cell.height()*AtlasScale));}
        painter.end();auto *node=static_cast<QSGSimpleTextureNode *>(root->firstChild());if(auto *texture=window()->createTextureFromImage(image)){node->setTexture(texture);node->setRect(boundingRect());}
    }
    return root;
}

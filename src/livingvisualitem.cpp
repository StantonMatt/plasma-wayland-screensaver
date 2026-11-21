// SPDX-License-Identifier: GPL-3.0-or-later
#include "livingvisualitem.h"
#include <QQuickWindow>
#include <QSGGeometryNode>
#include <QSGSimpleTextureNode>
#include <QSGTexture>
#include <QSGVertexColorMaterial>
#include <QThread>
#include <algorithm>
#include <cmath>
namespace
{
class DepthNode final : public QSGNode
{
public:
    QSGSimpleTextureNode* data = new QSGSimpleTextureNode;
    QSGGeometryNode* circles = new QSGGeometryNode;
    quint64 revision = 0;
    quint64 layoutGeneration = 0;
    struct Draw
    {
        vd_circle circle;
        double alpha;
    };
    std::vector<Draw> draw;
    DepthNode()
    {
        appendChildNode(data);
        appendChildNode(circles);
        auto* m = new QSGVertexColorMaterial;
        circles->setMaterial(m);
        circles->setFlag(QSGNode::OwnsMaterial);
    }
};
}
LivingVisualItem::LivingVisualItem(QQuickItem* parent)
    : QQuickItem(parent)
    , m_progress(std::make_shared<RenderProgress>())
{
    setFlag(ItemHasContents);
    setClip(true);
    const auto trackWindow = [this](QQuickWindow* w)
    {
        disconnect(m_renderConnection);
        if (!w)
            return;
        const auto progress = m_progress;
        m_renderConnection = connect(
            w, &QQuickWindow::afterRendering, w,
            [progress]
            {
                progress->renderedTimeline.store(
                    progress->timeline.load(std::memory_order_acquire), std::memory_order_release);
                progress->renderedLayout.store(
                    progress->layout.load(std::memory_order_acquire), std::memory_order_release);
                progress->renderedData.store(progress->data.load(std::memory_order_acquire), std::memory_order_release);
                progress->rendered.store(
                    progress->synchronized.load(std::memory_order_acquire), std::memory_order_release);
            },
            Qt::DirectConnection);
    };
    connect(this, &QQuickItem::windowChanged, this, trackWindow);
    // Parenting in QQuickItem's constructor may already have assigned the window.
    trackWindow(window());
}
void LivingVisualItem::setSample(VisualSampleLease sample)
{
    Q_ASSERT(QThread::currentThread() == thread());
    m_pending = std::move(sample);
    update();
}
quint64 LivingVisualItem::renderedSample() const
{
    return m_progress->rendered.load(std::memory_order_acquire);
}
quint64 LivingVisualItem::renderedLayout() const
{
    return m_progress->renderedLayout.load(std::memory_order_acquire);
}
quint64 LivingVisualItem::renderedDataRevision() const
{
    return m_progress->renderedData.load(std::memory_order_acquire);
}
qint64 LivingVisualItem::renderedTimelineNs() const
{
    return m_progress->renderedTimeline.load(std::memory_order_acquire);
}
QSGNode* LivingVisualItem::updatePaintNode(QSGNode* old, UpdatePaintNodeData*)
{
    // Only immutable ABI endpoints are accessed on the render thread. No world,
    // coordinator, Configuration, timer or QObject property is consulted here.
    const auto lease = m_pending;
    if (!lease)
    {
        delete old;
        return nullptr;
    }
    vd_sample_info info {};
    vd_endpoint lo {}, hi {};
    if (visuals_depth_sample_info(lease.get(), &info) || visuals_depth_sample_endpoint(lease.get(), 0, &lo)
        || visuals_depth_sample_endpoint(lease.get(), 1, &hi))
    {
        delete old;
        return nullptr;
    }
    auto* node = buildDepthNode(old, lease, info, lo, hi);
    m_progress->timeline.store(info.view.timeline_ns, std::memory_order_release);
    m_progress->layout.store(info.generations.layout, std::memory_order_release);
    m_progress->data.store(info.generations.data, std::memory_order_release);
    m_progress->synchronized.store(info.generations.sample + 1, std::memory_order_release);
    return node;
}
QSGNode* LivingVisualItem::buildDepthNode(
    QSGNode* old, const VisualSampleLease&, const vd_sample_info& info, const vd_endpoint& lo, const vd_endpoint& hi)
{
    auto* node = static_cast<DepthNode*>(old);
    if (!node)
        node = new DepthNode;
    const bool textured=hi.texture_bytes&&hi.texture_width&&hi.texture_height
        && hi.texture_size==size_t(hi.texture_width)*hi.texture_height*4;
    if(!textured) {
        if(node->data){node->removeChildNode(node->data);delete node->data;node->data=nullptr;}
        node->revision=0;node->layoutGeneration=0;
    } else {
        if(!node->data){node->data=new QSGSimpleTextureNode;node->prependChildNode(node->data);}
        if(hi.texture_revision!=node->revision||info.generations.layout!=node->layoutGeneration) {
            const QImage image(hi.texture_bytes,int(hi.texture_width),int(hi.texture_height),QImage::Format_RGBA8888);
            auto *texture=window()->createTextureFromImage(image.copy());
            auto *retired=node->data->texture();node->data->setOwnsTexture(false);
            node->data->setTexture(texture);delete retired;node->data->setOwnsTexture(true);
            node->data->setFiltering(QSGTexture::Linear);node->revision=hi.texture_revision;node->layoutGeneration=info.generations.layout;
        }
        const auto &bounds=info.world_bounds;
        node->data->setRect((bounds.x-info.view.viewport.x)*info.view.uniform_scale+info.view.offset_x,
            (bounds.y-info.view.viewport.y)*info.view.uniform_scale+info.view.offset_y,
            bounds.width*info.view.uniform_scale,bounds.height*info.view.uniform_scale);
    }
    auto& draw = node->draw;
    draw.clear();
    draw.reserve(lo.circle_count + hi.circle_count);
    for (size_t i = 0; i < lo.circle_count; ++i)
    {
        const auto& a = lo.circles[i];
        const vd_circle* b = nullptr;
        for (size_t j = 0; j < hi.circle_count; ++j)
            if (a.id == hi.circles[j].id && a.generation == hi.circles[j].generation)
            {
                b = &hi.circles[j];
                break;
            }
        auto circle = a;
        if (b)
        {
            circle.x += (b->x - a.x) * info.alpha;
            circle.y += (b->y - a.y) * info.alpha;
            circle.radius += float((b->radius - a.radius) * info.alpha);
        }
        draw.push_back({ circle, b ? 1. : 1. - info.alpha });
    }
    for (size_t i = 0; i < hi.circle_count; ++i)
    {
        const auto& b = hi.circles[i];
        bool exists = false;
        for (size_t j = 0; j < lo.circle_count; ++j)
            if (b.id == lo.circles[j].id && b.generation == lo.circles[j].generation)
                exists = true;
        if (!exists)
            draw.push_back({ b, info.alpha });
    }
    constexpr int sides = 40;
    auto* geometry = node->circles->geometry();
    const auto count = int(draw.size()) * sides * 3;
    if (!geometry)
    {
        geometry = new QSGGeometry(QSGGeometry::defaultAttributes_ColoredPoint2D(), count);
        geometry->setDrawingMode(QSGGeometry::DrawTriangles);
        node->circles->setGeometry(geometry);
        node->circles->setFlag(QSGNode::OwnsGeometry);
    }
    else
        geometry->allocate(count);
    auto* v = geometry->vertexDataAsColoredPoint2D();
    int index = 0;
    const auto& view = info.view;
    for (const auto& d : draw)
    {
        const auto& c = d.circle;
        const double x = (c.x - view.viewport.x) * view.uniform_scale + view.offset_x,
                     y = (c.y - view.viewport.y) * view.uniform_scale + view.offset_y;
        const double radius = c.radius * view.uniform_scale;
        const auto channel = [](double n) { return uchar(std::clamp(n * 255., 0., 255.)); };
        const double life
            = std::min(std::clamp((double(info.view.timeline_ns) - double(c.birth_ns)) / 200'000'000., 0., 1.),
                std::clamp((double(c.death_ns) - double(info.view.timeline_ns)) / 200'000'000., 0., 1.));
        const double alpha = c.rgba[3] * d.alpha * life;
        const auto vertex = [&](double px, double py)
        {
            v[index++].set(float(px), float(py), channel(c.rgba[0] * alpha), channel(c.rgba[1] * alpha),
                channel(c.rgba[2] * alpha), channel(alpha));
        };
        for (int n = 0; n < sides; ++n)
        {
            const double a = n * 2. * M_PI / sides, b = (n + 1) * 2. * M_PI / sides;
            vertex(x, y);
            vertex(x + radius * std::cos(a), y + radius * std::sin(a));
            vertex(x + radius * std::cos(b), y + radius * std::sin(b));
        }
    }
    node->circles->markDirty(QSGNode::DirtyGeometry);
    return node;
}

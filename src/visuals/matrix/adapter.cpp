// SPDX-License-Identifier: GPL-3.0-or-later
#include "alphabet.h"
#include "bolts.h"
#include "livingvisualitem.h"
#include "material.h"
#include "matrixmaterial.h"
#include "stillclock.h"
#include "visualdepthregistry.h"
#include <QQuickWindow>
#include <QSGGeometryNode>
#include <QSGOpacityNode>
#include <algorithm>
#include <array>
#include <bit>
#include <cstring>
#include <span>

namespace
{
class FieldNode final : public QSGGeometryNode
{
    RainFieldTexture m_texture;
    QSizeF m_size;

public:
    explicit FieldNode(QSGTexture* atlas)
    {
        setGeometry(new QSGGeometry(QSGGeometry::defaultAttributes_Point2D(), 4));
        geometry()->setDrawingMode(QSGGeometry::DrawTriangleStrip);
        setFlag(OwnsGeometry);
        auto* m = new RainMaterial(false);
        m->atlas = atlas;
        m->data = &m_texture;
        setMaterial(m);
        setFlag(OwnsMaterial);
    }
    void sync(const vd_rain_layer& v)
    {
        const QSizeF size(v.uniforms.viewport[2], v.uniforms.viewport[3]);
        if (size != m_size)
        {
            m_size = size;
            QSGGeometry::updateRectGeometry(geometry(), QRectF(QPointF(), size));
            markDirty(DirtyGeometry);
        }
        m_texture.sync(v);
        static_cast<RainMaterial*>(material())->uniforms = v.uniforms;
        markDirty(DirtyMaterial);
    }
};

class HeadsNode final : public QSGGeometryNode
{
    VisualDataTexture m_texture;
    std::array<QByteArray, 2> m_uploads;
    int m_uploadSlot = 0;
    size_t m_capacity = 0, m_count = 0;
    QSizeF m_size;
    uint64_t m_revision = UINT64_MAX;

public:
    explicit HeadsNode(QSGTexture* atlas)
    {
        setGeometry(new QSGGeometry(QSGGeometry::defaultAttributes_TexturedPoint2D(), 0));
        geometry()->setDrawingMode(QSGGeometry::DrawTriangles);
        geometry()->setVertexDataPattern(QSGGeometry::StaticPattern);
        setFlag(OwnsGeometry);
        auto* m = new RainMaterial(true);
        m->atlas = atlas;
        m->data = &m_texture;
        setMaterial(m);
        setFlag(OwnsMaterial);
    }
    void sync(std::span<const vd_rain_head> heads, uint64_t revision, const vd_rain_uniforms& u)
    {
        auto* m = static_cast<RainMaterial*>(material());
        m->uniforms = u;
        if (revision != m_revision)
        {
            m_revision = revision;
            const auto n = heads.size();
            const QSizeF size(u.viewport[2], u.viewport[3]);
            const auto capacity = std::max(m_capacity, std::bit_ceil(std::max(size_t(1), n)));
            if (capacity != m_capacity || size != m_size || (m_count == 0) != (n == 0))
            {
                m_capacity = capacity;
                m_size = size;
                geometry()->allocate(n ? int(capacity * 6) : 0);
                constexpr int corners[][2] = { { 0, 0 }, { 1, 0 }, { 0, 1 }, { 0, 1 }, { 1, 0 }, { 1, 1 } };
                auto* vertices = geometry()->vertexDataAsTexturedPoint2D();
                for (size_t i = 0; n && i < capacity; ++i)
                    for (const auto& c : corners)
                        (vertices++)->set(float(size.width()) * c[0], float(size.height()) * c[1], float(i), 0.f);
                geometry()->markVertexDataDirty();
                markDirty(DirtyGeometry);
            }
            m_count = n;
            auto& bytes = m_uploads[m_uploadSlot];
            m_uploadSlot = 1 - m_uploadSlot;
            bytes.resize(qsizetype(std::max(size_t(1), n) * sizeof(vd_rain_head)));
            if (n)
                std::memcpy(bytes.data(), heads.data(), n * sizeof(vd_rain_head));
            m_texture.setData(bytes, { 3, int(capacity) }, { 3, int(std::max(size_t(1), n)) });
        }
        m->uniforms.grid[3] = float(m_count);
        markDirty(DirtyMaterial);
    }
};

class FieldGroup final : public QSGOpacityNode
{
    std::array<FieldNode*, 3> m_fields {};
    HeadsNode* m_heads;
    RainBolts* m_bolts;
    std::vector<vd_rain_head> m_records = std::vector<vd_rain_head>(256);
    uint64_t m_revision = UINT64_MAX;
    size_t m_headCount = 0, m_recordCount = 0;

public:
    explicit FieldGroup(QQuickWindow* window, QSGTexture* atlas)
        : m_heads(new HeadsNode(atlas))
        , m_bolts(new RainBolts(window, atlas))
    {
        appendChildNode(m_heads);
        appendChildNode(m_bolts);
    }
    void sync(QSGTexture* atlas, const VisualSampleLease& lease, const vd_rain_still* still, uint64_t revision)
    {
        for (size_t i = 0; i < m_fields.size(); ++i)
        {
            vd_rain_layer view {};
            if (!visuals_rain_layer(lease.get(), still, i, &view) || !view.col_count || !view.row_count)
                continue;
            if (!m_fields[i])
            {
                m_fields[i] = new FieldNode(atlas);
                insertChildNodeBefore(m_fields[i], m_heads);
            }
            m_fields[i]->sync(view);
            if (i == 1) {
                if (m_revision != revision) {
                    m_revision = revision;
                    m_recordCount = visuals_rain_heads(lease.get(), still, m_records.data(), m_records.size());
                    if (m_recordCount > m_records.size()) {
                        m_records.resize(m_recordCount);
                        visuals_rain_heads(lease.get(), still, m_records.data(), m_recordCount);
                    }
                    // Rust appends cached bolt channels after the rain heads.
                    // Share one export and preserve rain compositing order.
                    m_headCount = m_recordCount;
                    while (m_headCount && m_records[m_headCount - 1].light[1] == 3.f)
                        --m_headCount;
                }
                const std::span<const vd_rain_head> records(m_records.data(), m_recordCount);
                m_heads->sync(records.first(m_headCount), revision, view.uniforms);
                m_bolts->sync(records.subspan(m_headCount), revision, view.uniforms);
            }
        }
    }
};
struct RainNode final : QSGNode
{
    MatrixAtlasTexture atlas;
    uint64_t layout = 0, epoch = 0;
    FieldGroup *current, *incoming = nullptr;
    using Still = std::unique_ptr<vd_rain_still, decltype(&visuals_rain_release_still)>;
    Still picture { nullptr, visuals_rain_release_still }, next { nullptr, visuals_rain_release_still };
    explicit RainNode(QQuickWindow* window, const QImage& image)
        : atlas(image)
        , current(new FieldGroup(window, &atlas))
    {
        appendChildNode(current);
    }
    ~RainNode() override
    {
        // Private bolt bindings must be gone before their atlas member.
        delete current;
        delete incoming;
    }
};

class RainItem final : public LivingVisualItem
{
    std::shared_ptr<RainStillClock> m_clock;
    bool m_scheduled = false;

public:
    using LivingVisualItem::LivingVisualItem;

protected:
    QSGNode* buildDepthNode(QSGNode* old, const VisualSampleLease& lease, const vd_sample_info& info,
        const vd_endpoint&, const vd_endpoint&) override
    {
        auto* root = static_cast<RainNode*>(old);
        if (root && info.generations.layout != root->layout)
        {
            delete root;
            root = nullptr;
        }
        if (!root)
        {
            if (window()->rendererInterface()->graphicsApi() == QSGRendererInterface::Software)
            {
                Q_EMIT gpuUnavailable();
                return nullptr;
            }
            const auto& atlas = rainAlphabetImage();
            if (!RainMaterial::supported(window(), atlas.size()))
            {
                Q_EMIT gpuUnavailable();
                return nullptr;
            }
            root = new RainNode(window(), atlas);
            root->layout = info.generations.layout;
            qInfo("Visuals Matrix: living phosphor field, paged SDF atlas and GPU heads");
        }
        vd_rain_layer mid {};
        if (!visuals_rain_layer(lease.get(), nullptr, 1, &mid))
            return root;
        if (mid.still && !m_scheduled)
        {
            m_scheduled = true;
            QMetaObject::invokeMethod(
                this,
                [this]
                {
                    m_clock = RainStillClock::acquire();
                    connect(&m_clock->refresh, &QTimer::timeout, this, [this] { update(); });
                    connect(&m_clock->dissolve, &QVariantAnimation::valueChanged, this, [this] { update(); });
                },
                Qt::QueuedConnection);
        }
        const quint64 epoch = m_clock ? m_clock->epoch : 0;
        const float mix = m_clock ? m_clock->mix : 0;
        if (mid.still && epoch != root->epoch)
        {
            root->next.reset(visuals_rain_still(lease.get(), epoch));
            if (root->incoming)
                delete root->incoming;
            root->incoming = new FieldGroup(window(), &root->atlas);
            root->appendChildNode(root->incoming);
            root->incoming->sync(&root->atlas, lease, root->next.get(), epoch);
            root->epoch = epoch;
        }
        if (root->incoming && mix >= 1.f)
        {
            delete root->current;
            root->current = root->incoming;
            root->incoming = nullptr;
            root->picture = std::move(root->next);
            root->current->setOpacity(1.);
        }
        root->current->sync(&root->atlas, lease, root->picture.get(), info.upper_tick);
        if (root->incoming)
        {
            root->current->setOpacity(1. - mix);
            root->incoming->setOpacity(mix);
        }
        return root;
    }
};
}
void registerDepthMatrixAdapter()
{
    VisualDepthRegistry::registerItemFactory(6, [](QQuickItem* parent) { return new RainItem(parent); });
}

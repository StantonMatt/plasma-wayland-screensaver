// SPDX-License-Identifier: GPL-3.0-or-later
#include "livingvisualitem.h"
#include "material.h"
#include "visualdepthregistry.h"
#include <QSGClipNode>
#include <QSGGeometryNode>
#include <bit>
#include <cmath>
#include <vector>

namespace {
class BallBatch final : public QSGGeometryNode {
public:
  BallBatch() {
    setGeometry(
        new QSGGeometry(QSGGeometry::defaultAttributes_TexturedPoint2D(), 0));
    geometry()->setDrawingMode(QSGGeometry::DrawTriangles);
    geometry()->setVertexDataPattern(QSGGeometry::StaticPattern);
    setFlag(OwnsGeometry);
    setMaterial(new BounceMaterial);
    setFlag(OwnsMaterial);
  }
  bool sync(const VisualSampleLease &lease, const vd_sample_info &info) {
    if (!visuals_depth_bounce_write(lease.get(), m_records.data(),
                                    m_records.size(), &m_batch)) {
      if (m_batch.count <= m_records.size() || m_batch.count > 8192)
        return false;
      m_records.resize(m_batch.count);
      if (!visuals_depth_bounce_write(lease.get(), m_records.data(),
                                      m_records.size(), &m_batch))
        return false;
    }
    const auto &v = info.view;
    auto *m = static_cast<BounceMaterial *>(material());
    const QSizeF size(v.viewport.width, v.viewport.height);
    const std::array<uint32_t, 3> counts{
        m_batch.trail_start, m_batch.sphere_start - m_batch.trail_start,
        m_batch.count - m_batch.sphere_start};
    auto capacities = m_capacities;
    for (size_t i = 0; i < counts.size(); ++i)
      if (counts[i] > capacities[i])
        capacities[i] = std::bit_ceil(counts[i]);
    if (m_size != size || m_capacities != capacities) {
      m_size = size;
      m_capacities = capacities;
      geometry()->allocate(6 * int(capacities[0] + capacities[1] + capacities[2]));
      auto *pos = geometry()->vertexDataAsTexturedPoint2D();
      constexpr int corners[][2] = {{0, 0}, {1, 0}, {0, 1},
                                    {0, 1}, {1, 0}, {1, 1}};
      for (size_t pass = 0; pass < 3; ++pass)
        for (uint32_t row = 0; row < capacities[pass]; ++row)
          for (const auto &corner : corners)
            (pos++)->set(float(size.width() * corner[0]),
                         float(size.height() * corner[1]), float(row),
                         pass == 1 ? 0.f : (pass == 0 ? -1.f : -2.f));
      geometry()->markVertexDataDirty();
      markDirty(DirtyGeometry);
    }
    m_textureRows =
        std::max(m_textureRows, std::bit_ceil(std::max(1u, m_batch.count)));
    const auto rows = std::max(1u, m_batch.count);
    m->records.setData(
        QByteArray(reinterpret_cast<const char *>(m_records.data()),
                   qsizetype(rows * sizeof(vd_bounce_record))),
        {4, int(m_textureRows)}, {4, int(rows)});
    m->trails.sync(m_batch, info.generations.layout != m_layout ||
                                v.timeline_ns < m_time);
    m_layout = info.generations.layout;
    m_time = v.timeline_ns;
    auto &u = m->uniforms;
    u[0] = float(size.width());
    u[1] = float(size.height());
    u[2] = float(m_textureRows);
    u[4] = float(v.viewport.x - v.offset_x / v.uniform_scale);
    u[5] = float(v.viewport.y - v.offset_y / v.uniform_scale);
    u[6] = float(v.uniform_scale);
    u[7] = (std::abs(v.offset_x) > 0.01 || std::abs(v.offset_y) > 0.01) ? 1.f
                                                                        : 0.f;
    const auto &cabinet = info.world_bounds;
    u[20] = float(cabinet.x);
    u[21] = float(cabinet.y);
    u[22] = float(cabinet.x + cabinet.width);
    u[23] = float(cabinet.y + cabinet.height);
    for (size_t i = 0; i < counts.size(); ++i)
      u[8 + i] = float(counts[i]);
    u[11] = m_batch.trail_half_width;
    std::copy_n(m_batch.top, 4, u.begin() + 12);
    std::copy_n(m_batch.bottom, 4, u.begin() + 16);
    markDirty(DirtyMaterial);
    return true;
  }

private:
  QSizeF m_size;
  std::array<uint32_t, 3> m_capacities{};
  uint32_t m_textureRows = 1;
  uint64_t m_layout = 0;
  int64_t m_time = -1;
  std::vector<vd_bounce_record> m_records = std::vector<vd_bounce_record>(512);
  vd_bounce_batch m_batch{};
};
class BounceItem final : public LivingVisualItem {
public:
  using LivingVisualItem::LivingVisualItem;

protected:
  QSGNode *buildDepthNode(QSGNode *old, const VisualSampleLease &lease,
                          const vd_sample_info &info, const vd_endpoint &,
                          const vd_endpoint &) override {
    auto *clip = static_cast<QSGClipNode *>(old);
    if (!clip) {
      if (!BounceMaterial::supported(window())) {
        Q_EMIT gpuUnavailable();
        return nullptr;
      }
      qInfo("Visuals Bounce: living GPU spheres and trail rings");
      clip = new QSGClipNode;
      clip->setIsRectangular(true);
      clip->appendChildNode(new BallBatch);
    }
    const auto &w = info.world_bounds;
    const auto &v = info.view;
    QRectF rect((w.x - v.viewport.x) * v.uniform_scale + v.offset_x,
                (w.y - v.viewport.y) * v.uniform_scale + v.offset_y,
                w.width * v.uniform_scale, w.height * v.uniform_scale);
    // Only the rail shader emits light into the contain bands.
    if (std::abs(v.offset_x) > 0.01 || std::abs(v.offset_y) > 0.01)
      rect.adjust(-12.5, -12.5, 12.5, 12.5);
    if (clip->clipRect() != rect)
      clip->setClipRect(rect);
    if (!static_cast<BallBatch *>(clip->firstChild())->sync(lease, info)) {
      delete clip;
      Q_EMIT gpuUnavailable();
      return nullptr;
    }
    return clip;
  }
};
} // namespace
void registerDepthBounceAdapter() {
  VisualDepthRegistry::registerItemFactory(
      7, [](QQuickItem *parent) { return new BounceItem(parent); });
}

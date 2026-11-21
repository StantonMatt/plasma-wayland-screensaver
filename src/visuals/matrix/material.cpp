// SPDX-License-Identifier: GPL-3.0-or-later
#include "material.h"
#include <QFile>
#include <QQuickWindow>
#include <QSGMaterialShader>
#include <cstring>

static void initializeRainShaders() { Q_INIT_RESOURCE(rain_shaders); }
namespace {
struct Pack {
  QShader vert, frag;
};
const Pack &shaders(bool heads) {
  static const auto packs = [] {
    initializeRainShaders();
    const auto load = [](const QString &file) {
      QFile f(QStringLiteral(":/depth/rain/") + file + QStringLiteral(".qsb"));
      return f.open(QIODevice::ReadOnly) ? QShader::fromSerialized(f.readAll())
                                         : QShader();
    };
    return std::array<Pack, 2>{{{load(QStringLiteral("field.vert")),
                                 load(QStringLiteral("field.frag"))},
                                {load(QStringLiteral("heads.vert")),
                                 load(QStringLiteral("heads.frag"))}}};
  }();
  return packs[heads];
}
class Shader final : public QSGMaterialShader {
public:
  explicit Shader(bool heads) {
    setShader(VertexStage, shaders(heads).vert);
    setShader(FragmentStage, shaders(heads).frag);
  }
  bool updateUniformData(RenderState &state, QSGMaterial *material,
                         QSGMaterial *) override {
    const auto *m = static_cast<RainMaterial *>(material);
    auto *data = state.uniformData()->data();
    if (state.isMatrixDirty())
      std::memcpy(data, state.combinedMatrix().constData(), 64);
    auto uniforms = m->uniforms;
    uniforms.light[3] = state.opacity();
    std::memcpy(data + 64, &uniforms, sizeof(uniforms));
    return true;
  }
  void updateSampledImage(RenderState &state, int binding, QSGTexture **texture,
                          QSGMaterial *material, QSGMaterial *) override {
    auto *m = static_cast<RainMaterial *>(material);
    *texture = binding == 1 ? m->data : m->atlas;
    (*texture)->commitTextureOperations(state.rhi(),
                                        state.resourceUpdateBatch());
  }
};
} // namespace
RainFieldTexture::RainFieldTexture() { setFiltering(Nearest); }
void RainFieldTexture::sync(const vd_rain_layer &v) {
  const QSize size(2 * int(v.row_count), int(v.col_count));
  const bool fresh = size != m_size || m_stamp == 0 || v.stamp < m_stamp;
  if (size != m_size) {
    m_texture.reset();
    m_size = size;
  }
  if (!fresh && v.stamp == m_stamp)
    return;
  // If a node was synchronized twice before QRhi consumed our last batch,
  // its pending subregions must stay intact.
  if (fresh)
    m_changes.clear();
  auto upload = [&](uint32_t col, uint32_t count, uint32_t first,
                    uint32_t last) {
    const int length = int(last - first) * 8;
    QByteArray bytes(length * int(count), Qt::Uninitialized);
    const auto *source = v.cells + (size_t(col) * v.rows + first) * 8;
    if (first == 0 && last == v.rows)
      std::memcpy(bytes.data(), source, size_t(bytes.size()));
    else {
      auto *target = bytes.data();
      for (uint32_t k = 0; k < count; ++k)
        std::memcpy(target + k * length, source + size_t(k) * v.rows * 8,
                    size_t(length));
    }
    QRhiTextureSubresourceUploadDescription change(bytes);
    change.setSourceSize({length / 4, int(count)});
    change.setDestinationTopLeft(
        {2 * int(first - v.row_first), int(col - v.col_first)});
    m_changes.emplace_back(0, 0, change);
  };
  if (fresh)
    upload(v.col_first, v.col_count, v.row_first, v.row_first + v.row_count);
  else {
    // QRhi submits a subimage transfer per rectangle. Coalesce adjacent
    // dirty texture rows, including their interior cell spans, within a
    // bounded transfer. Split at every clean row and viewport boundary.
    uint32_t firstCol = 0, ncols = 0, firstRow = 0, lastRow = 0;
    auto flush = [&] {
      if (ncols)
        upload(firstCol, ncols, firstRow, lastRow);
      ncols = 0;
    };
    const bool consecutive = v.stamp == m_stamp + 1;
    for (uint32_t c = v.col_first; c < v.col_first + v.col_count; ++c) {
      const auto dirty = v.dirty[c];
      const auto lo =
          consecutive ? std::max(v.row_first, uint32_t(dirty.lo)) : v.row_first;
      const auto hi = consecutive ? std::min(v.row_first + v.row_count,
                                             uint32_t(dirty.hi) + 1)
                                  : v.row_first + v.row_count;
      if (dirty.stamp <= m_stamp || hi <= lo) {
        flush();
        continue;
      }
      if (ncols &&
          (std::max(lastRow, hi) - std::min(firstRow, lo)) * (ncols + 1) > 4096)
        flush();
      if (!ncols) {
        firstCol = c;
        firstRow = lo;
        lastRow = hi;
      }
      firstRow = std::min(firstRow, lo);
      lastRow = std::max(lastRow, hi);
      ++ncols;
    }
    flush();
  }
  m_stamp = v.stamp;
}
void RainFieldTexture::commitTextureOperations(
    QRhi *rhi, QRhiResourceUpdateBatch *updates) {
  if (!m_texture) {
    m_texture.reset(rhi->newTexture(QRhiTexture::RGBA8, m_size));
    if (!m_texture->create()) {
      m_texture.reset();
      return;
    }
  }
  if (m_changes.empty())
    return;
  QRhiTextureUploadDescription description;
  description.setEntries(m_changes.cbegin(), m_changes.cend());
  updates->uploadTexture(m_texture.get(), description);
  m_changes.clear();
}
RainMaterial::RainMaterial(bool headBatch) : heads(headBatch) {
  setFlag(Blending);
  setFlag(RequiresFullMatrix);
  setFlag(NoBatching);
}
QSGMaterialType *RainMaterial::type() const {
  static QSGMaterialType types[2];
  return &types[heads];
}
QSGMaterialShader *
RainMaterial::createShader(QSGRendererInterface::RenderMode) const {
  return new Shader(heads);
}
int RainMaterial::compare(const QSGMaterial *other) const {
  return other == this                                   ? 0
         : std::less<const QSGMaterial *>{}(this, other) ? -1
                                                         : 1;
}

bool RainMaterial::supported(QQuickWindow *window, QSize atlas) {
  auto *ri = window->rendererInterface();
  auto *rhi = static_cast<QRhi *>(
      ri->getResource(window, QSGRendererInterface::RhiResource));
  if (!rhi || rhi->backend() == QRhi::Null || atlas.isEmpty())
    return false;
  const int limit = rhi->resourceLimit(QRhi::TextureSizeMax);
  if (limit < std::max({4096, atlas.width(), atlas.height()}) ||
      !rhi->isTextureFormatSupported(QRhiTexture::R8) ||
      !rhi->isTextureFormatSupported(QRhiTexture::RGBA32F))
    return false;
  auto *target = static_cast<QRhiRenderTarget *>(
      ri->getResource(window, QSGRendererInterface::RhiRedirectRenderTarget));
  if (!target) {
    auto *swap = static_cast<QRhiSwapChain *>(
        ri->getResource(window, QSGRendererInterface::RhiSwapchainResource));
    if (swap)
      target = swap->currentFrameRenderTarget();
  }
  if (!target || !target->renderPassDescriptor())
    return false;
  std::unique_ptr<QRhiBuffer> buf(
      rhi->newBuffer(QRhiBuffer::Dynamic, QRhiBuffer::UniformBuffer, 256));
  std::unique_ptr<QRhiTexture> sdf(rhi->newTexture(QRhiTexture::R8, {16, 16}));
  std::unique_ptr<QRhiSampler> sampler(rhi->newSampler(
      QRhiSampler::Nearest, QRhiSampler::Nearest, QRhiSampler::None,
      QRhiSampler::ClampToEdge, QRhiSampler::ClampToEdge));
  if (!buf->create() || !sdf->create() || !sampler->create())
    return false;
  for (bool heads : {false, true}) {
    const auto &pack = shaders(heads);
    if (!pack.vert.isValid() || !pack.frag.isValid())
      return false;
    std::unique_ptr<QRhiTexture> records(rhi->newTexture(
        heads ? QRhiTexture::RGBA32F : QRhiTexture::RGBA8, {4, 4}));
    if (!records->create())
      return false;
    std::unique_ptr<QRhiShaderResourceBindings> set(
        rhi->newShaderResourceBindings());
    const auto vert = QRhiShaderResourceBinding::VertexStage,
               frag = QRhiShaderResourceBinding::FragmentStage;
    set->setBindings(
        {QRhiShaderResourceBinding::uniformBuffer(0, vert | frag, buf.get()),
         QRhiShaderResourceBinding::sampledTexture(
             1, heads ? vert : frag, records.get(), sampler.get()),
         QRhiShaderResourceBinding::sampledTexture(2, frag, sdf.get(),
                                                   sampler.get())});
    if (!set->create())
      return false;
    QRhiVertexInputLayout layout;
    layout.setBindings({{heads ? 16u : 8u}});
    if (heads)
      layout.setAttributes({{0, 0, QRhiVertexInputAttribute::Float2, 0},
                            {0, 1, QRhiVertexInputAttribute::Float2, 8}});
    else
      layout.setAttributes({{0, 0, QRhiVertexInputAttribute::Float2, 0}});
    std::unique_ptr<QRhiGraphicsPipeline> pipe(rhi->newGraphicsPipeline());
    pipe->setShaderStages({{QRhiShaderStage::Vertex, pack.vert},
                           {QRhiShaderStage::Fragment, pack.frag}});
    pipe->setVertexInputLayout(layout);
    pipe->setShaderResourceBindings(set.get());
    pipe->setRenderPassDescriptor(target->renderPassDescriptor());
    pipe->setSampleCount(target->sampleCount());
    QRhiGraphicsPipeline::TargetBlend blend;
    blend.enable = true;
    pipe->setTargetBlends({blend});
    if (!pipe->create())
      return false;
  }
  return true;
}

// SPDX-License-Identifier: GPL-3.0-or-later
#include "material.h"
#include <QFile>
#include <QQuickWindow>
#include <cstring>

static void initializeBounceShaders() { Q_INIT_RESOURCE(depth_bounce_shaders); }
namespace {
struct Pack {
  QShader vertex, fragment;
};
const Pack &shaders() {
  static const Pack pack = [] {
    initializeBounceShaders();
    const auto load = [](const char *name) {
      QFile file(QStringLiteral(":/depth/bounce/") + QString::fromLatin1(name));
      return file.open(QIODevice::ReadOnly)
                 ? QShader::fromSerialized(file.readAll())
                 : QShader{};
    };
    return Pack{load("bounce.vert.qsb"), load("bounce.frag.qsb")};
  }();
  return pack;
}
class Shader final : public QSGMaterialShader {
public:
  Shader() {
    setShader(VertexStage, shaders().vertex);
    setShader(FragmentStage, shaders().fragment);
  }
  bool updateUniformData(RenderState &state, QSGMaterial *material,
                         QSGMaterial *) override {
    auto *m = static_cast<BounceMaterial *>(material);
    char *dest = state.uniformData()->data();
    if (state.isMatrixDirty())
      std::memcpy(dest, state.combinedMatrix().constData(), 64);
    m->uniforms[3] = state.opacity();
    std::memcpy(dest + 64, m->uniforms.data(), sizeof(m->uniforms));
    return true;
  }
  void updateSampledImage(RenderState &state, int binding, QSGTexture **output,
                          QSGMaterial *material, QSGMaterial *) override {
    auto *m = static_cast<BounceMaterial *>(material);
    if (binding == 1)
      *output = &m->records;
    else if (binding == 2)
      *output = &m->trails;
    else
      return;
    (*output)->commitTextureOperations(state.rhi(),
                                       state.resourceUpdateBatch());
  }
};
} // namespace

BounceTrails::BounceTrails() {
  setFiltering(Nearest);
  setHorizontalWrapMode(ClampToEdge);
  setVerticalWrapMode(ClampToEdge);
}

void BounceTrails::sync(const vd_bounce_batch &batch, bool invalidate) {
  // Several synchronizations may precede one render. Retain all dirty spans
  // until QRhi consumes them. Every span owns its copy of the leased knots.
  if (invalidate) {
    m_serial.fill(0);
    m_pending.clear();
  }
  for (int row = 0; row < 61; ++row) {
    const auto &ring = batch.rings[row];
    if (!ring.knots) {
      m_serial[row] = 0;
      continue;
    }
    uint64_t start = m_serial[row] <= ring.serial ? m_serial[row] : 0;
    if (ring.serial - start > 32)
      start = ring.serial - 32;
    while (start < ring.serial) {
      const int x = int(start % 32);
      const int count = int(std::min<uint64_t>(32 - x, ring.serial - start));
      QRhiTextureSubresourceUploadDescription span(QByteArray(
          reinterpret_cast<const char *>(ring.knots + x),
          count * qsizetype(sizeof(*ring.knots))));
      span.setDestinationTopLeft({2 * x, row});
      span.setSourceSize({2 * count, 1});
      m_pending.append(QRhiTextureUploadEntry(0, 0, span));
      start += count;
    }
    m_serial[row] = ring.serial;
  }
}
void BounceTrails::commitTextureOperations(QRhi *rhi,
                                           QRhiResourceUpdateBatch *updates) {
  if (!m_texture) {
    m_texture.reset(rhi->newTexture(QRhiTexture::RGBA32F, textureSize()));
    if (!m_texture->create()) {
      m_texture.reset();
      return;
    }
  }
  if (!m_pending.isEmpty()) {
    QRhiTextureUploadDescription upload;
    upload.setEntries(m_pending.cbegin(), m_pending.cend());
    updates->uploadTexture(m_texture.get(), upload);
    m_pending.clear();
  }
}
BounceMaterial::BounceMaterial() {
  setFlag(Blending);
  setFlag(RequiresFullMatrix);
  setFlag(NoBatching);
}
QSGMaterialType *BounceMaterial::type() const {
  static QSGMaterialType type;
  return &type;
}
QSGMaterialShader *
BounceMaterial::createShader(QSGRendererInterface::RenderMode) const {
  return new Shader;
}
int BounceMaterial::compare(const QSGMaterial *other) const {
  return other == this
             ? 0
             : (std::less<const QSGMaterial *>{}(this, other) ? -1 : 1);
}
bool BounceMaterial::supported(QQuickWindow *window) {
  if (!window || !shaders().vertex.isValid() || !shaders().fragment.isValid())
    return false;
  auto *ri = window->rendererInterface();
  auto *rhi = static_cast<QRhi *>(
      ri->getResource(window, QSGRendererInterface::RhiResource));
  if (!rhi || rhi->backend() == QRhi::Null ||
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
  std::unique_ptr<QRhiBuffer> uniform(
      rhi->newBuffer(QRhiBuffer::Dynamic, QRhiBuffer::UniformBuffer, 160));
  std::unique_ptr<QRhiTexture> texture(
      rhi->newTexture(QRhiTexture::RGBA32F, {4, 512}));
  std::unique_ptr<QRhiSampler> sampler(rhi->newSampler(
      QRhiSampler::Nearest, QRhiSampler::Nearest, QRhiSampler::None,
      QRhiSampler::ClampToEdge, QRhiSampler::ClampToEdge));
  if (!uniform->create() || !texture->create() || !sampler->create())
    return false;
  std::unique_ptr<QRhiShaderResourceBindings> bindings(
      rhi->newShaderResourceBindings());
  const auto vertex = QRhiShaderResourceBinding::VertexStage;
  bindings->setBindings(
      {QRhiShaderResourceBinding::uniformBuffer(
           0, vertex | QRhiShaderResourceBinding::FragmentStage, uniform.get()),
       QRhiShaderResourceBinding::sampledTexture(1, vertex, texture.get(),
                                                 sampler.get()),
       QRhiShaderResourceBinding::sampledTexture(
           2, vertex | QRhiShaderResourceBinding::FragmentStage, texture.get(),
           sampler.get())});
  if (!bindings->create())
    return false;
  QRhiVertexInputLayout layout;
  layout.setBindings({{16}});
  layout.setAttributes({{0, 0, QRhiVertexInputAttribute::Float2, 0},
                        {0, 1, QRhiVertexInputAttribute::Float2, 8}});
  std::unique_ptr<QRhiGraphicsPipeline> pipeline(rhi->newGraphicsPipeline());
  pipeline->setShaderStages({{QRhiShaderStage::Vertex, shaders().vertex},
                             {QRhiShaderStage::Fragment, shaders().fragment}});
  pipeline->setVertexInputLayout(layout);
  pipeline->setShaderResourceBindings(bindings.get());
  pipeline->setRenderPassDescriptor(target->renderPassDescriptor());
  pipeline->setSampleCount(target->sampleCount());
  QRhiGraphicsPipeline::TargetBlend blend;
  blend.enable = true;
  pipeline->setTargetBlends({blend});
  return pipeline->create();
}

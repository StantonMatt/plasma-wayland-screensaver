// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include "visualmaterial.h"
#include "visuals_depth_bounce.h"
#include <QPointF>
#include <array>
#include <rhi/qrhi.h>

class BounceTrails final : public QSGTexture {
public:
  BounceTrails();
  qint64 comparisonKey() const override { return qint64(quintptr(this)); }
  QRhiTexture *rhiTexture() const override { return m_texture.get(); }
  QSize textureSize() const override { return {64, 61}; }
  bool hasAlphaChannel() const override { return false; }
  bool hasMipmaps() const override { return false; }
  void sync(const vd_bounce_batch &, bool invalidate);
  void commitTextureOperations(QRhi *, QRhiResourceUpdateBatch *) override;

private:
  std::unique_ptr<QRhiTexture> m_texture;
  std::array<uint64_t, 61> m_serial{};
  QList<QRhiTextureUploadEntry> m_pending;
};

class BounceMaterial final : public QSGMaterial {
public:
  BounceMaterial();
  QSGMaterialType *type() const override;
  QSGMaterialShader *
      createShader(QSGRendererInterface::RenderMode) const override;
  int compare(const QSGMaterial *) const override;
  static bool supported(QQuickWindow *);
  VisualDataTexture records;
  BounceTrails trails;
  std::array<float, 24> uniforms{};
};

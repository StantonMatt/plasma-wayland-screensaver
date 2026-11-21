// SPDX-License-Identifier: GPL-3.0-or-later
#include "glyphatlas.h"
#include "visuals_glyph_sdf.h"
#include <QCryptographicHash>
#include <QDataStream>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QPainter>
#include <QPainterPath>
#include <QSaveFile>
#include <algorithm>
#include <array>
#include <cstring>

QRect GlyphAtlas::tile(quint16 id) {
  return {int(id % Columns) * Cell, int(id / Columns) * Cell, Cell, Cell};
}

GlyphAtlas::GlyphAtlas(const QVector<Glyph> &glyphs,
                       const QVector<QRawFont> &faces, const QRawFont &fallback,
                       const QString &cacheFile) {
  if (glyphs.isEmpty() || !fallback.isValid())
    return;
  const int last =
      std::max_element(glyphs.begin(), glyphs.end(),
                       [](const auto &a, const auto &b) { return a.id < b.id; })
          ->id;
  m_image = QImage(Columns * Cell, (1 + last / PageCapacity) * Rows * Cell,
                   QImage::Format_Alpha8);
  if (m_image.isNull())
    return;
  QVector<QPainterPath> outlines;
  outlines.reserve(glyphs.size());
  // The rendered outlines form the cache key, including fallback choices.
  // A font upgrade or a changed ID-to-outline mapping invalidates the file.
  QByteArray geometry;
  QDataStream keyStream(&geometry, QIODevice::WriteOnly);
  keyStream << QByteArray("pvs-glyph-sdf-r8-48-8-v1") << QByteArray(qVersion())
            << m_image.size();
  for (const auto &glyph : glyphs) {
    const auto *requested = glyph.face >= 0 && glyph.face < faces.size()
                                ? &faces.at(glyph.face)
                                : nullptr;
    const bool supported = requested && requested->isValid() &&
                           requested->supportsCharacter(glyph.codepoint);
    const auto &face = supported ? *requested : fallback;
    const auto cp = supported ? glyph.codepoint : glyph.fallback;
    const auto ids = face.glyphIndexesForString(QString::fromUcs4(&cp, 1));
    outlines.push_back(ids.isEmpty() || !ids.front()
                           ? QPainterPath()
                           : face.pathForGlyph(ids.front()));
    if (!cacheFile.isEmpty())
      keyStream << glyph.id << outlines.back();
  }
  const auto key =
      QCryptographicHash::hash(geometry, QCryptographicHash::Sha256);
  const auto pixelsHash = [&] {
    return QCryptographicHash::hash(
        QByteArrayView(reinterpret_cast<const char *>(m_image.constBits()),
                       m_image.sizeInBytes()),
        QCryptographicHash::Sha256);
  };
  QFile cache(cacheFile);
  if (!cacheFile.isEmpty() && cache.open(QIODevice::ReadOnly)) {
    if (cache.size() == 2 * key.size() + m_image.sizeInBytes() &&
        cache.read(key.size()) == key) {
      const auto checksum = cache.read(key.size());
      if (cache.read(reinterpret_cast<char *>(m_image.bits()),
                     m_image.sizeInBytes()) == m_image.sizeInBytes() &&
          checksum == pixelsHash())
        return;
    }
    cache.close();
  }

  m_image.fill(0);
  QImage coverage(Cell, Cell, QImage::Format_Alpha8);
  std::array<unsigned char, Cell * Cell> sdf;
  for (qsizetype index = 0; index < glyphs.size(); ++index) {
    const auto &glyph = glyphs[index];
    const auto &path = outlines[index];
    if (path.isEmpty())
      continue;
    const auto bounds = path.boundingRect();
    coverage.fill(0);
    QPainter p(&coverage);
    p.setRenderHint(QPainter::Antialiasing);
    p.setPen(Qt::NoPen);
    p.setBrush(Qt::white);
    // Keep punctuation on its baseline and preserve the font's proportions.
    const qreal fit = std::min({1.0, 35.0 / std::max(1.0, bounds.width()),
                                35.0 / std::max(1.0, bounds.height())});
    p.translate(Cell * .5 - bounds.center().x() * fit,
                std::clamp(34., 4. - bounds.top() * fit,
                           Cell - 4. - bounds.bottom() * fit));
    p.scale(fit, fit);
    p.drawPath(path);
    p.end();
    if (!visuals_glyph_sdf(coverage.constBits(), sdf.data(), Cell, 8.0f))
      continue;
    const auto rect = tile(glyph.id);
    for (int row = 0; row < Cell; ++row)
      std::memcpy(m_image.scanLine(rect.y() + row) + rect.x(),
                  sdf.data() + row * Cell, Cell);
  }
  if (!cacheFile.isEmpty() &&
      QDir().mkpath(QFileInfo(cacheFile).absolutePath())) {
    // Readers see a complete atlas even when two processes cold-start together.
    QSaveFile output(cacheFile);
    if (output.open(QIODevice::WriteOnly) && output.write(key) == key.size() &&
        output.write(pixelsHash()) == key.size() &&
        output.write(reinterpret_cast<const char *>(m_image.constBits()),
                     m_image.sizeInBytes()) == m_image.sizeInBytes())
      output.commit();
  }
}

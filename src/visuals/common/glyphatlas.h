// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <QImage>
#include <QRawFont>
#include <QVector>

// Stable u16 IDs address 16 x 16 tiles per page. No atlas coordinate depends
// on the visual's logical glyph size or on the active display's device ratio.
class GlyphAtlas {
public:
  static constexpr int Cell = 48, Columns = 16, Rows = 16,
                       PageCapacity = Columns * Rows;
  struct Glyph {
    quint16 id;
    char32_t codepoint;
    int face;
    char32_t fallback;
  };
  GlyphAtlas(const QVector<Glyph> &, const QVector<QRawFont> &faces,
             const QRawFont &fallback, const QString &cacheFile = {});
  const QImage &image() const { return m_image; }
  int pages() const { return m_image.height() / (Rows * Cell); }
  static QRect tile(quint16 id);

private:
  QImage m_image;
};

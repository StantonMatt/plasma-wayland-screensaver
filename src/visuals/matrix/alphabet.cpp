// SPDX-License-Identifier: GPL-3.0-or-later
#include "alphabet.h"
#include <QFileInfo>
#include <QFontDatabase>
#include <QPainterPath>
#include <QStandardPaths>
#include <algorithm>
#include <array>
#include <cmath>

void roundRainBraille(QImage &image, const QRawFont &font) {
  if (image.format() != QImage::Format_Alpha8 || !font.isValid())
    return;
  std::array<QRectF, 8> dots;
  for (int bit = 0; bit < 8; ++bit) {
    const char32_t cp = 0x2800 + (1 << bit);
    if (!font.supportsCharacter(cp))
      return;
    const auto ids = font.glyphIndexesForString(QString::fromUcs4(&cp, 1));
    if (ids.isEmpty() || !ids.front())
      return;
    dots[bit] = font.pathForGlyph(ids.front()).boundingRect();
    if (dots[bit].isEmpty())
      return;
  }
  for (int i = 0; i < 96; ++i) {
    const unsigned pattern = 1 + (i * 73) % 255;
    const char32_t cp = 0x2800 + pattern;
    if (!font.supportsCharacter(cp))
      continue; // The shared atlas has already installed the numeric fallback.
    const auto ids = font.glyphIndexesForString(QString::fromUcs4(&cp, 1));
    if (ids.isEmpty() || !ids.front())
      continue;
    const auto bounds = font.pathForGlyph(ids.front()).boundingRect();
    const qreal fit = std::min({1., 35. / std::max(1., bounds.width()),
                               35. / std::max(1., bounds.height())});
    const QPointF offset(24. - bounds.center().x() * fit,
                         std::clamp(34., 4. - bounds.top() * fit,
                                    44. - bounds.bottom() * fit));
    const auto tile = GlyphAtlas::tile(225 + i);
    if (!image.rect().contains(tile))
      continue;
    // Sample the exact disc distance, rather than distance-transforming a
    // rasterised font dot. Its round contour survives magnification to G's
    // 84 px near layer without inheriting the font's square dot outline.
    for (int y = 0; y < 48; ++y) {
      auto *row = image.scanLine(tile.y() + y) + tile.x();
      for (int x = 0; x < 48; ++x) {
        qreal distance = -8.;
        for (int bit = 0; bit < 8; ++bit) {
          if (!(pattern & (1u << bit)))
            continue;
          const auto centre = dots[bit].center() * fit + offset;
          const auto radius =
              0.5 * std::min(dots[bit].width(), dots[bit].height()) * fit;
          distance = std::max(
              distance,
              radius - std::hypot(x + .5 - centre.x(), y + .5 - centre.y()));
        }
        row[x] = uchar(
            std::clamp(std::lround(255. * (.5 + distance / 16.)), 0l, 255l));
      }
    }
  }
}

namespace {
QRawFont face(const QString &family, const QString &pinned) {
#ifdef PVS_ENABLE_VISUAL_CAPTURE
  if (qEnvironmentVariableIsSet("VISUALS_REQUIRE_GPU") &&
      QFileInfo::exists(pinned))
    return QRawFont(pinned, 33., QFont::PreferNoHinting);
#else
  Q_UNUSED(pinned);
#endif
  if (!QFontDatabase::families().contains(family))
    return {};
  QFont font(family);
  font.setPixelSize(33);
  font.setHintingPreference(QFont::PreferNoHinting);
  return QRawFont::fromFont(font);
}
QImage build() {
  auto mono = face(
      QStringLiteral("DejaVu Sans Mono"),
      QStringLiteral("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"));
  auto kana = face(
      QStringLiteral("Noto Sans CJK JP"),
      QStringLiteral("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"));
  auto dots =
      face(QStringLiteral("DejaVu Sans"),
           QStringLiteral("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"));
  auto fallback = mono;
  if (!fallback.isValid()) {
    auto f = QFontDatabase::systemFont(QFontDatabase::FixedFont);
    f.setPixelSize(33);
    fallback = QRawFont::fromFont(f);
  }
  QVector<GlyphAtlas::Glyph> glyphs;
  const auto add = [&](int start, const QString &text, int font) {
    for (const auto c : text.toUcs4()) {
      glyphs.append(
          {quint16(start), char32_t(c), font, char32_t('0' + start % 10)});
      ++start;
    }
  };
  add(1, QStringLiteral("0123456789·:"), 0);
  add(17, QStringLiteral("0123456789ABCDEF<>[]{}"), 0);
  QString katakana, letters = QStringLiteral("ABCDEFGHIJKLMNOPQRSTUVWXYZ"),
                    braille;
  const QString small = QStringLiteral("ァィゥェォッャュョヮヵヶ");
  for (int cp = 0x30a1; cp <= 0x30f6; ++cp)
    if (!small.contains(QChar(cp)))
      katakana.append(QChar(cp));
  for (int i = 0; i < 24; ++i)
    letters.append(QChar(0x3b1 + i + (i >= 17 ? 1 : 0)));
  // Include sparse and clustered cells, with no blank Braille slot.
  for (int i = 0; i < 96; ++i)
    braille.append(QChar(0x2801 + (i * 73) % 255));
  add(49, katakana, 1);
  add(145, letters, 0);
  add(209, QStringLiteral("│╱╲┼╳╎┊─▚▞"), 0);
  add(225, braille, 2);
  add(321, QStringLiteral("∑∂∞≈≠±√∫"), 0);
  const auto cache =
      QStandardPaths::writableLocation(QStandardPaths::CacheLocation);
  auto image = GlyphAtlas(
      glyphs, {mono, kana, dots}, fallback,
      cache.isEmpty() ? QString() : cache + QStringLiteral("/matrix-sdf.bin")).image();
  roundRainBraille(image, dots);
  return image;
}
} // namespace
const QImage &rainAlphabetImage() {
  static const QImage image = build();
  return image;
}

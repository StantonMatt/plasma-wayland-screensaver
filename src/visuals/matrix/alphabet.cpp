// SPDX-License-Identifier: GPL-3.0-or-later
#include "alphabet.h"
#include <QFileInfo>
#include <QFontDatabase>
#include <QStandardPaths>

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
GlyphAtlas build() {
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
  return GlyphAtlas(
      glyphs, {mono, kana, dots}, fallback,
      cache.isEmpty() ? QString() : cache + QStringLiteral("/matrix-sdf.bin"));
}
} // namespace
const GlyphAtlas &rainAlphabet() {
  static const GlyphAtlas atlas = build();
  return atlas;
}

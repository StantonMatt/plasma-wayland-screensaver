// SPDX-License-Identifier: GPL-3.0-or-later
#include "visuals/common/glyphatlas.h"
#include "visuals/matrix/alphabet.h"
#include "visuals/matrix/stillclock.h"
#include <QFileInfo>
#include <QPainterPath>
#include <QTemporaryDir>
#include <QTimeZone>
#include <QtTest>
#include <cmath>

class AtlasTest : public QObject {
  Q_OBJECT
private Q_SLOTS:
  void brailleUsesDiscDistancesAndKeepsFontFallback() {
    const QRawFont font(
        QStringLiteral("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"),
        33., QFont::PreferNoHinting);
    QVERIFY(font.isValid());
    QImage image(768, 1536, QImage::Format_Alpha8);
    image.fill(37);
    const auto original = image.copy();
    roundRainBraille(image, QRawFont());
    QCOMPARE(image, original);
    roundRainBraille(image, font);
    QCOMPARE(image.copy(GlyphAtlas::tile(1)), original.copy(GlyphAtlas::tile(1)));
    QCOMPARE(image.copy(GlyphAtlas::tile(321)),
             original.copy(GlyphAtlas::tile(321)));
    // Slot 225 is one dot. Equal radial distances must have equal SDF values,
    // including diagonal samples that the square font outline filled before.
    const auto tile = image.copy(GlyphAtlas::tile(225));
    const auto oldTile = GlyphAtlas({{225, 0x2801, 0, U'0'}}, {font}, font)
                             .image().copy(GlyphAtlas::tile(225));
    const char32_t cp = 0x2801;
    const auto ids = font.glyphIndexesForString(QString::fromUcs4(&cp, 1));
    const auto bounds = font.pathForGlyph(ids.front()).boundingRect();
    const double radius = .5 * std::min(bounds.width(), bounds.height());
    const QPointF centre(24., 34. + bounds.center().y());
    double maxError = 0., oldMaxError = 0.;
    for (int y = 4; y < 18; ++y) {
      for (int x = 17; x < 31; ++x) {
        const double expected = std::clamp(
            radius - std::hypot(x + .5 - centre.x(), y + .5 - centre.y()), -8., 8.);
        const double actual = (tile.constScanLine(y)[x] / 255. - .5) * 16.;
        const double old = (oldTile.constScanLine(y)[x] / 255. - .5) * 16.;
        maxError = std::max(maxError, std::abs(expected - actual));
        oldMaxError = std::max(oldMaxError, std::abs(expected - old));
        QVERIFY2(std::abs(expected - actual) <= 8. / 255. + 1e-6,
                 "Braille SDF must describe a circle");
      }
    }
    qInfo() << "Braille disc distance error in atlas pixels: before"
            << oldMaxError << "after" << maxError;
    // All 96 addresses keep their Unicode bit pattern, including bottom-row
    // bits 6 and 7. A four-neighbour flood fill counts isolated discs.
    for (int i = 0; i < 96; ++i) {
      auto dots = image.copy(GlyphAtlas::tile(225 + i));
      int components = 0;
      for (int y = 0; y < 48; ++y) {
        for (int x = 0; x < 48; ++x) {
          if (dots.constScanLine(y)[x] <= 128)
            continue;
          ++components;
          QList<QPoint> pending{{x, y}};
          while (!pending.isEmpty()) {
            const auto p = pending.takeLast();
            if (!dots.rect().contains(p) ||
                dots.constScanLine(p.y())[p.x()] <= 128)
              continue;
            dots.scanLine(p.y())[p.x()] = 0;
            for (const auto step :
                 {QPoint(1, 0), QPoint(-1, 0), QPoint(0, 1), QPoint(0, -1)})
              pending.append(p + step);
          }
        }
      }
      QCOMPARE(components, qPopulationCount(unsigned(1 + (i * 73) % 255)));
    }
  }
  void stillAlarmAndDissolve() {
    auto left = RainStillClock::acquire();
    auto right = RainStillClock::acquire();
    QCOMPARE(left.get(), right.get());
    QCOMPARE(left->dissolve.state(), QAbstractAnimation::Stopped);
    QCOMPARE(left->refresh.interval(), 300000);
    QVERIFY(QMetaObject::invokeMethod(&left->refresh, "timeout",
                                      Qt::DirectConnection));
    QCOMPARE(right->epoch, 1u);
    QCOMPARE(right->dissolve.duration(), 8000);
    left->dissolve.setCurrentTime(4000);
    QCOMPARE(right->mix, 0.5f);
    left->dissolve.setCurrentTime(8000);
    QCOMPARE(right->mix, 1.f);
    QCOMPARE(right->dissolve.state(), QAbstractAnimation::Stopped);
    left.reset();
    right.reset();
    QCOMPARE(RainStillClock::acquire()->epoch, 0u);
  }
  void cachedAtlasMatchesFreshOutlinesAndRecoversFromDamage() {
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    const auto name = dir.filePath(QStringLiteral("sdf.bin"));
    const QRawFont mono(
        QStringLiteral("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"),
        33., QFont::PreferNoHinting);
    QVERIFY(mono.isValid());
    const QVector<GlyphAtlas::Glyph> original{{1, U'3', 0, U'0'},
                                              {257, U'9', 0, U'0'}};
    const auto expected = GlyphAtlas(original, {mono}, mono).image();
    QCOMPARE(GlyphAtlas(original, {mono}, mono, name).image(), expected);
    QFile saved(name);
    QVERIFY(saved.open(QIODevice::ReadWrite));
    const auto oldTime =
        QDateTime::fromMSecsSinceEpoch(1000000000000LL, QTimeZone::UTC);
    QVERIFY(saved.setFileTime(oldTime, QFileDevice::FileModificationTime));
    saved.close();
    QCOMPARE(GlyphAtlas(original, {mono}, mono, name).image(), expected);
    QCOMPARE(QFileInfo(name).lastModified().toMSecsSinceEpoch(),
             oldTime.toMSecsSinceEpoch());
    QVERIFY(saved.open(QIODevice::ReadWrite));
    QVERIFY(saved.seek(saved.size() - 1));
    QCOMPARE(saved.write("X", 1), 1);
    saved.close();
    QCOMPARE(GlyphAtlas(original, {mono}, mono, name).image(), expected);
    auto changed = original;
    changed[0].codepoint = U'B';
    const auto replacement = GlyphAtlas(changed, {mono}, mono, name).image();
    QCOMPARE(replacement, GlyphAtlas(changed, {mono}, mono).image());
    QVERIFY(replacement != expected);
  }
  void multiplePagesAndMissingFace() {
    const QRawFont mono(
        QStringLiteral("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"),
        33., QFont::PreferNoHinting);
    QVERIFY(mono.isValid());
    const GlyphAtlas atlas({{1, U'9', 0, U'0'},
                            {257, U'あ', 1, U'9'},
                            {321, U'∑', 0, U'9'},
                            {322, U'あ', 9, U'9'}},
                           {mono, QRawFont()}, mono);
    QCOMPARE(atlas.pages(), 2);
    const auto first = atlas.image().copy(GlyphAtlas::tile(1));
    QCOMPARE(first, atlas.image().copy(GlyphAtlas::tile(257)));
    QCOMPARE(first, atlas.image().copy(GlyphAtlas::tile(322)));
    QVERIFY(first != atlas.image().copy(GlyphAtlas::tile(321)));
    QVERIFY(*std::max_element(first.constBits(),
                              first.constBits() + first.sizeInBytes()) > 128);
  }
};
QTEST_MAIN(AtlasTest)
#include "test_glyphatlas.moc"

// SPDX-License-Identifier: GPL-3.0-or-later
#include "visuals/common/glyphatlas.h"
#include "visuals/matrix/stillclock.h"
#include <QFileInfo>
#include <QTemporaryDir>
#include <QTimeZone>
#include <QtTest>

class AtlasTest : public QObject {
  Q_OBJECT
private Q_SLOTS:
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

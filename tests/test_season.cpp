// SPDX-License-Identifier: GPL-3.0-or-later
#include "season.h"
#include <QTest>
#include <QTimeZone>

class SeasonTest final : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void windowAndOverrides()
    {
        QCOMPARE(seasonFor(QDate(2026,10,23), true, {}), Season::None);
        QCOMPARE(seasonFor(QDate(2026,10,24), true, {}), Season::Halloween);
        QCOMPARE(seasonFor(QDate(2026,11,1), true, u"auto"), Season::Halloween);
        QCOMPARE(seasonFor(QDate(2026,11,2), true, {}), Season::None);
        QCOMPARE(seasonFor(QDate(2026,10,31), false, {}), Season::None);
        QCOMPARE(seasonFor(QDate(2026,10,31), true, u"off"), Season::None);
        QCOMPARE(seasonFor(QDate(2026,1,1), false, u"halloween"), Season::Halloween);
        QCOMPARE(seasonFor(QDate(2026,1,1), true, u"2026-10-24"), Season::Halloween);
        QCOMPARE(seasonFor(QDate(2026,10,31), true, u"2026-11-02"), Season::None);
        QCOMPARE(seasonFor(QDate(2026,1,1), false, u"2026-10-24"), Season::None);
        QCOMPARE(seasonFor(QDate(2026,10,31), true, u"bad-date"), Season::Halloween);
        QCOMPARE(seasonFor({}, true, {}), Season::None);
    }
    void wrappingWindowAndFirstMatch()
    {
        const Season future = static_cast<Season>(2);
        const SeasonWindow windows[] = {{future,12,20,1,1},{Season::Halloween,12,25,12,31}};
        for (const QDate date : {QDate(2026,12,20),QDate(2026,12,31),QDate(2027,1,1)})
            QCOMPARE(seasonInWindows(date, windows), future);
        QCOMPARE(seasonInWindows(QDate(2026,12,19), windows), Season::None);
        QCOMPARE(seasonInWindows(QDate(2027,1,2), windows), Season::None);
    }
    void midnightAcrossDst()
    {
        const QTimeZone zone("Europe/Berlin");
        QVERIFY(zone.isValid());
        QCOMPARE(millisecondsToSeasonMidnight(QDateTime(QDate(2026,3,29), QTime(0,0), zone)), 23*3600000LL+2000);
        QCOMPARE(millisecondsToSeasonMidnight(QDateTime(QDate(2026,10,25), QTime(0,0), zone)), 25*3600000LL+2000);
        QCOMPARE(millisecondsToSeasonMidnight(QDateTime(QDate(2026,10,23), QTime(23,59,59), zone)), 3000LL);
    }
};
QTEST_GUILESS_MAIN(SeasonTest)
#include "test_season.moc"

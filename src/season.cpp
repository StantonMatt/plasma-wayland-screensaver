// SPDX-License-Identifier: GPL-3.0-or-later
#include "season.h"
#include <QTimeZone>

Season seasonInWindows(QDate date, std::span<const SeasonWindow> windows)
{
    if (!date.isValid()) return Season::None;
    const int day = date.month() * 100 + date.day();
    for (const auto &window : windows) {
        const int from = window.fromMonth * 100 + window.fromDay;
        const int to = window.toMonth * 100 + window.toDay;
        if (from <= to ? day >= from && day <= to : day >= from || day <= to)
            return window.season;
    }
    return Season::None;
}

Season seasonFor(QDate date, bool enabled, QStringView override)
{
    if (override == u"off") return Season::None;
    if (override == u"halloween") return Season::Halloween;
    if (!enabled) return Season::None;
    if (!override.isEmpty() && override != u"auto") {
        const QDate pretend = QDate::fromString(override.toString(), Qt::ISODate);
        // Invalid overrides fall back to the real local date.
        if (pretend.isValid() && pretend.toString(Qt::ISODate) == override) date = pretend;
    }
    static constexpr SeasonWindow windows[] = {{Season::Halloween, 10, 24, 11, 1}};
    return seasonInWindows(date, windows);
}

qint64 millisecondsToSeasonMidnight(const QDateTime &now)
{
    // Construct in the same local zone, rather than adding 24 hours: DST days
    // may be 23/25 hours long. Qt resolves skipped/ambiguous local midnights.
    const QDateTime midnight(now.date().addDays(1), QTime(0, 0, 2), now.timeZone());
    return qMax<qint64>(1, now.msecsTo(midnight));
}

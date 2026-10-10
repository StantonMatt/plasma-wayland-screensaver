// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once

#include <QDateTime>
#include <QStringView>
#include <span>

enum class Season : quint8 { None = 0, Halloween = 1 }; // 2 reserved for a future festival
struct SeasonWindow {
    Season season;
    int fromMonth, fromDay, toMonth, toDay;
};
Season seasonInWindows(QDate date, std::span<const SeasonWindow> windows);
Season seasonFor(QDate date, bool enabled, QStringView override);
qint64 millisecondsToSeasonMidnight(const QDateTime &now);

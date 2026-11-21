// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <QTimer>
#include <QVariantAnimation>
#include <memory>

// A shared five-minute alarm keeps the photographed instant and transition
// aligned across outputs. Qt's animation driver runs only during the dissolve.
class RainStillClock
{
public:
    QTimer refresh;
    QVariantAnimation dissolve;
    quint64 epoch = 0;
    float mix = 0;
    RainStillClock()
    {
        refresh.setInterval(300000);
        refresh.setTimerType(Qt::PreciseTimer);
        dissolve.setDuration(8000);
        dissolve.setStartValue(0.);
        dissolve.setEndValue(1.);
        QObject::connect(&refresh, &QTimer::timeout, &refresh,
            [this]
            {
                ++epoch;
                mix = 0;
                dissolve.start();
            });
        QObject::connect(
            &dissolve, &QVariantAnimation::valueChanged, &refresh, [this](const QVariant& v) { mix = v.toFloat(); });
        refresh.start();
    }
    static std::shared_ptr<RainStillClock> acquire()
    {
        static std::weak_ptr<RainStillClock> active;
        auto clock = active.lock();
        if (!clock)
        {
            clock = std::make_shared<RainStillClock>();
            active = clock;
        }
        return clock;
    }
};

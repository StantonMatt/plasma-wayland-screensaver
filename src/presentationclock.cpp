// SPDX-License-Identifier: GPL-3.0-or-later
#include "presentationclock.h"

#include <QQuickWindow>
#include <QCoreApplication>
#include <QFile>
#include <QPointer>
#include <QTimer>
#include <QVariant>
#include <QDebug>
#include <mutex>
#include <vector>
#include <time.h>
#include <unistd.h>
#include <QScreen>

#include <algorithm>
#include <chrono>

namespace {
qint64 traceNow()
{
    timespec value{};
    clock_gettime(CLOCK_MONOTONIC, &value);
    return qint64(value.tv_sec) * 1'000'000'000 + value.tv_nsec;
}

struct TraceRow {
    bool resource = false;
    int window = 0;
    qint64 timestamp = 0, interval = 0, tick = 0, sync = 0, render = 0;
    qint64 steps = -1, ticks = 0, rss = 0, cpu = 0;
};

// No file I/O or formatting on the render thread. A five-minute two-output
// run fits in the reserved buffer; write once after the windows stop rendering.
class FrameTraceSink final : public QObject
{
public:
    explicit FrameTraceSink(const QString &path) : QObject(qApp), file(path)
    {
        if (!file.open(QIODevice::WriteOnly | QIODevice::Truncate)) {
            qFatal("Cannot open PVS_FRAME_TRACE output");
        }
        rows.reserve(150'000);
        sampleResources();
        auto *timer = new QTimer(this);
        timer->setInterval(1000);
        connect(timer, &QTimer::timeout, this, [this] { sampleResources(); });
        timer->start();
        connect(qApp, &QCoreApplication::aboutToQuit, this, [this, timer] {
            timer->stop();
            sampleResources();
            flush();
        });
    }
    ~FrameTraceSink() override { flush(); }

    void sampleResources()
    {
        QFile statm(QStringLiteral("/proc/self/statm"));
        qint64 rss = 0;
        if (statm.open(QIODevice::ReadOnly)) {
            const auto fields = statm.readAll().simplified().split(' ');
            if (fields.size() > 1) rss = fields[1].toLongLong() * sysconf(_SC_PAGESIZE);
        }
        timespec cpu{};
        clock_gettime(CLOCK_PROCESS_CPUTIME_ID, &cpu);
        TraceRow row;
        row.resource = true;
        row.timestamp = traceNow();
        row.rss = rss;
        row.cpu = qint64(cpu.tv_sec) * 1'000'000'000 + cpu.tv_nsec;
        std::lock_guard lock(mutex);
        rows.push_back(row);
    }
    void flush()
    {
        std::lock_guard lock(mutex);
        if (!file.isOpen()) return;
        QByteArray data("kind,window,timestamp_ns,interval_ns,gui_tick_ns,sync_ns,render_ns,simulation_steps,tick_callbacks,rss_bytes,cpu_ns\n");
        for (const auto &r : rows) {
            data += r.resource ? "resource," : "frame,";
            for (qint64 value : {qint64(r.window), r.timestamp, r.interval, r.tick,
                                r.sync, r.render, r.steps, r.ticks, r.rss, r.cpu}) {
                data += QByteArray::number(value);
                data += ',';
            }
            data.chop(1);
            data += '\n';
        }
        if (file.write(data) != data.size() || !file.flush()) {
            qCritical() << "Incomplete frame trace:" << file.errorString();
        }
        file.close();
    }
    QFile file;
    std::mutex mutex;
    std::vector<TraceRow> rows;
    int nextWindow = 0;
};

FrameTraceSink *traceSink()
{
    static FrameTraceSink *sink = nullptr;
    if (!sink) sink = new FrameTraceSink(QString::fromLocal8Bit(qgetenv("PVS_FRAME_TRACE")));
    return sink;
}
}

class PresentationTraceState
{
public:
    FrameTraceSink *sink = nullptr;
    QPointer<QObject> simulationSource; // Only read on the GUI thread.
    bool simulationTracked = false; // Protected by the sink mutex.
    TraceRow pending, frame;
    qint64 syncStart = 0, renderStart = 0, previousSwap = 0;
};

PresentationClock::PresentationClock(QQuickWindow *window, int targetFrameRate, QObject *parent)
    : QObject(parent)
    , m_window(window)
    , m_targetFrameRate(targetFrameRate)
{
    if (!qEnvironmentVariableIsEmpty("PVS_FRAME_TRACE")) {
        m_trace = std::make_shared<PresentationTraceState>();
        const auto trace = m_trace;
        trace->sink = traceSink();
        trace->frame.window = ++trace->sink->nextWindow;
        trace->pending.steps = 0;
        qInfo() << "Frame trace window" << trace->frame.window << window->objectName()
                << "size" << window->size() << "target FPS" << targetFrameRate
                << "refresh Hz" << (window->screen() ? window->screen()->refreshRate() : 0);
        connect(window, &QQuickWindow::beforeSynchronizing, this, [trace] {
            const qint64 now = traceNow();
            std::lock_guard lock(trace->sink->mutex);
            trace->syncStart = now;
            trace->frame.tick = trace->pending.tick;
            trace->frame.ticks = trace->pending.ticks;
            trace->frame.steps = trace->simulationTracked ? trace->pending.steps : -1;
            trace->pending.tick = trace->pending.ticks = trace->pending.steps = 0;
            trace->frame.sync = trace->frame.render = 0;
        }, Qt::DirectConnection);
        connect(window, &QQuickWindow::afterSynchronizing, this, [trace] {
            trace->frame.sync = traceNow() - trace->syncStart;
        }, Qt::DirectConnection);
        connect(window, &QQuickWindow::beforeRendering, this, [trace] {
            trace->renderStart = traceNow();
        }, Qt::DirectConnection);
        connect(window, &QQuickWindow::afterRendering, this, [trace] {
            trace->frame.render = traceNow() - trace->renderStart;
        }, Qt::DirectConnection);
        connect(window, &QQuickWindow::frameSwapped, this, [trace] {
            const qint64 now = traceNow();
            trace->frame.timestamp = now;
            trace->frame.interval = trace->previousSwap ? now - trace->previousSwap : 0;
            trace->previousSwap = now;
            std::lock_guard lock(trace->sink->mutex);
            trace->sink->rows.push_back(trace->frame);
        }, Qt::DirectConnection);
    }
    m_wakeTimer.setSingleShot(true);
    m_wakeTimer.setTimerType(Qt::PreciseTimer);
    connect(&m_wakeTimer, &QChronoTimer::timeout, this, &PresentationClock::presentNextFrame);
    connect(m_window, &QQuickWindow::frameSwapped, this,
            &PresentationClock::handleFrameSwapped, Qt::QueuedConnection);
}

PresentationClock::~PresentationClock() = default;

void PresentationClock::setTraceSimulationSource(QObject *source)
{
    if (m_trace) {
        std::lock_guard lock(m_trace->sink->mutex);
        m_trace->simulationSource = source;
        m_trace->simulationTracked = source != nullptr;
    }
}

void PresentationClock::setRunning(bool running)
{
    if (m_running == running) {
        return;
    }
    m_running = running;
    m_wakeTimer.stop();
    m_elapsed.invalidate();
    if (m_running) {
        m_elapsed.start();
        if (m_targetFrameRate <= 0) {
            // Bootstrap by dirtying animated state once. Subsequent automatic
            // ticks are chained exclusively from completed presentations.
            QMetaObject::invokeMethod(this, [this] {
                const qreal refreshRate = m_window->screen()
                    ? m_window->screen()->refreshRate() : 60.0;
                tickAndRequestUpdate(1.0 / std::max(1.0, refreshRate));
            }, Qt::QueuedConnection);
        } else {
            scheduleNextFrame();
        }
    }
}

void PresentationClock::handleFrameSwapped()
{
    if (m_running && m_targetFrameRate <= 0) {
        tickAndRequestUpdate();
    }
}

void PresentationClock::presentNextFrame()
{
    if (!m_running) {
        return;
    }
    tickAndRequestUpdate();
    scheduleNextFrame();
}

void PresentationClock::tickAndRequestUpdate(qreal fallbackSeconds)
{
    qreal deltaSeconds = m_elapsed.isValid()
        ? std::min(m_elapsed.nsecsElapsed() / 1'000'000'000.0, 0.10) : 0.0;
    m_elapsed.restart();
    if (deltaSeconds <= 0.0001) {
        deltaSeconds = fallbackSeconds;
    }
    if (deltaSeconds > 0.0) {
        if (m_trace) {
            QObject *source = m_trace->simulationSource.data();
            const qreal before = source ? source->property("simulationTime").toDouble() : 0;
            const qreal step = source ? source->property("physicsStepSeconds").toDouble() : 0;
            const qint64 start = traceNow();
            Q_EMIT frameTick(deltaSeconds);
            const qint64 duration = traceNow() - start;
            const qreal after = source ? source->property("simulationTime").toDouble() : 0;
            std::lock_guard lock(m_trace->sink->mutex);
            m_trace->pending.tick += duration;
            ++m_trace->pending.ticks;
            if (step > 0) m_trace->pending.steps += std::max<qint64>(0, qRound64((after - before) / step));
        } else {
            Q_EMIT frameTick(deltaSeconds);
        }
    }
    m_window->requestUpdate();
}

void PresentationClock::scheduleNextFrame()
{
    const qreal refreshRate = m_window->screen() ? m_window->screen()->refreshRate() : 60.0;
    const qreal requestedRate = std::min<qreal>(m_targetFrameRate, refreshRate);
    // QChronoTimer provides sub-millisecond scheduling for 144–240 Hz output.
    // Fixed caps use a precise wakeup, but motion still advances from measured
    // elapsed time so delayed frames do not slow the apparent motion.
    const auto period = std::chrono::duration_cast<std::chrono::nanoseconds>(
        std::chrono::duration<qreal>(1.0 / std::max(1.0, requestedRate)));
    m_wakeTimer.setInterval(period);
    m_wakeTimer.start();
}

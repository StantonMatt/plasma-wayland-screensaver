// SPDX-License-Identifier: GPL-3.0-or-later
#include "presentationclock.h"

#include <QQuickWindow>
#include <QCoreApplication>
#include <QFile>
#include <QPointer>
#include <QTimer>
#include <QThread>
#include <QVariant>
#include <QDebug>
#include <mutex>
#include <vector>
#include <time.h>
#include <unistd.h>
#include <QScreen>

#include <algorithm>
#include <chrono>
#include <cmath>

qreal PresentationPacing::validRefreshRate(qreal refreshRate)
{
    return std::isfinite(refreshRate) && refreshRate >= 1.0 ? refreshRate : 60.0;
}

int PresentationPacing::refreshDivisor(int targetFrameRate, qreal refreshRate)
{
    const qreal refresh = validRefreshRate(refreshRate);
    return targetFrameRate > 0 ? std::max(1, int(std::ceil(refresh / targetFrameRate))) : 1;
}

long double PresentationPacing::periodNanoseconds(int targetFrameRate, qreal refreshRate)
{
    return 1'000'000'000.0L * refreshDivisor(targetFrameRate, refreshRate)
        / validRefreshRate(refreshRate);
}

PresentationPacing::Schedule PresentationPacing::schedule(
    qint64 nowNanoseconds, long double nextTargetNanoseconds,
    int targetFrameRate, qreal refreshRate, std::optional<qint64> phaseAnchorNanoseconds)
{
    const long double period = periodNanoseconds(targetFrameRate, refreshRate);
    if (!phaseAnchorNanoseconds) {
        // A late timer consumes just the most recent due slot. The next deadline
        // stays on the original grid even when synchronous work takes too long.
        if (nowNanoseconds > nextTargetNanoseconds) {
            nextTargetNanoseconds += std::floor((nowNanoseconds - nextTargetNanoseconds) / period) * period;
        }
        const qint64 deadline = std::llround(nextTargetNanoseconds);
        const qint64 wake = std::max(nowNanoseconds, deadline);
        return {wake, wake, nextTargetNanoseconds + period};
    }

    const long double refreshPeriod = 1'000'000'000.0L / validRefreshRate(refreshRate);
    const long double anchor = *phaseAnchorNanoseconds;
    // Never select a predicted vsync that passed during a GUI stall.
    const long double firstSlot = std::max(1.0L,
        std::floor((nowNanoseconds - anchor) / refreshPeriod) + 1.0L);
    // Round each absolute ideal target to its nearest vsync (ties go forward).
    // Skip missed targets in one operation, rather than emitting catch-up ticks.
    const long double firstTarget = anchor + (firstSlot - 0.5L) * refreshPeriod;
    // One nanosecond of tolerance makes exact half-refresh ties stable despite
    // floating-point arithmetic and quantized swap timestamps.
    constexpr long double roundingTolerance = 1.0L;
    if (nextTargetNanoseconds < firstTarget - roundingTolerance) {
        nextTargetNanoseconds += std::ceil(
            (firstTarget - nextTargetNanoseconds - roundingTolerance) / period) * period;
    }
    const long double slot = std::max(firstSlot,
        std::floor((nextTargetNanoseconds - anchor + roundingTolerance) / refreshPeriod + 0.5L));
    const qint64 presentation = std::llround(anchor + slot * refreshPeriod);
    // Start work one refresh before presentation, leaving time for simulation,
    // scene-graph sync and rendering. Auto/full-rate starts at the prior slot;
    // a fixed cap sleeps through the unused refreshes without requesting frames.
    const qint64 wake = std::max(nowNanoseconds,
        qint64(std::llround(anchor + (slot - 1.0L) * refreshPeriod)));
    return {wake, presentation, nextTargetNanoseconds + period};
}

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
    qint64 wakeDeadline = 0, tickTimestamp = 0, requestTimestamp = 0;
    qint64 presentation = 0, swapCallback = 0, fallbacks = 0;
};

// Shared independently of QObject lifetime: a direct render callback already
// in flight may outlive its clock or the application's GUI-owned file sink.
struct FrameTraceBuffer {
    FrameTraceBuffer() { rows.reserve(150'000); }
    std::mutex mutex;
    std::vector<TraceRow> rows;
    bool accepting = true; // Protected by mutex, including shutdown.
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
        std::lock_guard lock(buffer->mutex);
        if (buffer->accepting) buffer->rows.push_back(row);
    }
    void flush()
    {
        std::lock_guard lock(buffer->mutex);
        if (!file.isOpen()) return;
        buffer->accepting = false;
        QByteArray data("kind,window,timestamp_ns,interval_ns,gui_tick_ns,sync_ns,render_ns,simulation_steps,tick_callbacks,rss_bytes,cpu_ns,wake_deadline_ns,tick_timestamp_ns,request_timestamp_ns,predicted_presentation_ns,swap_callback_ns,timer_fallbacks\n");
        for (const auto &r : buffer->rows) {
            data += r.resource ? "resource," : "frame,";
            for (qint64 value : {qint64(r.window), r.timestamp, r.interval, r.tick,
                                r.sync, r.render, r.steps, r.ticks, r.rss, r.cpu,
                                r.wakeDeadline, r.tickTimestamp, r.requestTimestamp,
                                r.presentation, r.swapCallback, r.fallbacks}) {
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
    // File, sampling timer and window numbering are exclusively GUI-owned.
    QFile file;
    const std::shared_ptr<FrameTraceBuffer> buffer = std::make_shared<FrameTraceBuffer>();
    int nextWindow = 0;
};

FrameTraceSink *traceSink()
{
    static QPointer<FrameTraceSink> sink;
    if (!sink) sink = new FrameTraceSink(QString::fromLocal8Bit(qgetenv("PVS_FRAME_TRACE")));
    return sink.data();
}
}

class PresentationTraceState
{
public:
    std::shared_ptr<FrameTraceBuffer> buffer;
    QPointer<QObject> simulationSource; // Only read on the GUI thread.
    bool simulationTracked = false; // Protected by the buffer mutex.
    TraceRow pending; // GUI writes, render sync consumes, under buffer mutex.
    // Initialized before connecting/showing the window, then render-thread
    // owned for all scene-graph phases. No GUI handler reads these fields.
    TraceRow frame;
    qint64 syncStart = 0, renderStart = 0, previousSwap = 0;
};

PresentationClock::PresentationClock(QQuickWindow *window, int targetFrameRate, QObject *parent)
    : QObject(parent)
    , m_swapTimestamp(std::make_shared<std::atomic<qint64>>(0))
    , m_window(window)
    , m_targetFrameRate(targetFrameRate)
{
    const auto swapTimestamp = m_swapTimestamp;
    connect(window, &QQuickWindow::frameSwapped, this, [swapTimestamp] {
        swapTimestamp->store(traceNow(), std::memory_order_relaxed);
    }, Qt::DirectConnection);
    if (!qEnvironmentVariableIsEmpty("PVS_FRAME_TRACE")) {
        m_trace = std::make_shared<PresentationTraceState>();
        const auto trace = m_trace;
        auto *sink = traceSink();
        trace->buffer = sink->buffer;
        trace->frame.window = ++sink->nextWindow;
        trace->pending.steps = 0;
        qInfo() << "Frame trace window" << trace->frame.window << window->objectName()
                << "size" << window->size() << "target FPS" << targetFrameRate
                << "refresh Hz" << (window->screen() ? window->screen()->refreshRate() : 0);
        // The renderer interface must be queried on the render thread. Window
        // format is fixed before exposure; hide/release quiesces rendering
        // before native-surface destruction, so these lifetime-bound reads are
        // stable without touching any mutable GUI pacing/simulation state.
        connect(window, &QQuickWindow::sceneGraphInitialized, this, [window, trace] {
            const auto api = window->rendererInterface()->graphicsApi();
            const auto name = api == QSGRendererInterface::Vulkan ? "vulkan"
                : api == QSGRendererInterface::OpenGL ? "opengl" : "other";
            qInfo().noquote() << "Frame trace initialized window" << trace->frame.window
                << "graphics API" << name << "render thread"
                << (QThread::currentThread() != qApp->thread())
                << "swap interval" << window->format().swapInterval();
        }, Qt::DirectConnection);
        connect(window, &QQuickWindow::beforeSynchronizing, this, [trace] {
            const qint64 now = traceNow();
            std::lock_guard lock(trace->buffer->mutex);
            trace->syncStart = now;
            trace->frame.tick = trace->pending.tick;
            trace->frame.ticks = trace->pending.ticks;
            trace->frame.steps = trace->simulationTracked ? trace->pending.steps : -1;
            trace->frame.wakeDeadline = trace->pending.wakeDeadline;
            trace->frame.tickTimestamp = trace->pending.tickTimestamp;
            trace->frame.requestTimestamp = trace->pending.requestTimestamp;
            trace->frame.presentation = trace->pending.presentation;
            trace->frame.swapCallback = trace->pending.swapCallback;
            trace->frame.fallbacks = trace->pending.fallbacks;
            trace->pending.tick = trace->pending.ticks = trace->pending.steps = 0;
            trace->pending.wakeDeadline = trace->pending.tickTimestamp = 0;
            trace->pending.requestTimestamp = trace->pending.presentation = 0;
            trace->pending.swapCallback = trace->pending.fallbacks = 0;
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
            std::lock_guard lock(trace->buffer->mutex);
            if (trace->buffer->accepting) trace->buffer->rows.push_back(trace->frame);
        }, Qt::DirectConnection);
    }
    m_wakeTimer.setSingleShot(true);
    m_wakeTimer.setTimerType(Qt::PreciseTimer);
    connect(&m_wakeTimer, &QChronoTimer::timeout, this, &PresentationClock::presentNextFrame);
    connect(m_window, &QQuickWindow::frameSwapped, this,
            &PresentationClock::handleFrameSwapped, Qt::QueuedConnection);
}

PresentationClock::~PresentationClock() = default;

void PresentationClock::setTargetFrameRate(int targetFrameRate)
{
    if (m_targetFrameRate == targetFrameRate) return;
    m_targetFrameRate = targetFrameRate;
    if (m_running) {
        const qint64 now = traceNow();
        m_nextTargetNanoseconds = now
            + PresentationPacing::periodNanoseconds(m_targetFrameRate, m_refreshRate);
        if (!m_waitingForSwap) scheduleNextFrame();
    }
}

void PresentationClock::setTraceSimulationSource(QObject *source)
{
    if (m_trace) {
        std::lock_guard lock(m_trace->buffer->mutex);
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
    m_waitingForSwap = false;
    m_lastSwapNanoseconds.reset();
    m_phaseAnchorNanoseconds.reset();
    m_lastSwapCallbackNanoseconds = m_timerFallbacks = 0;
    if (m_running) {
        m_runStartNanoseconds = traceNow();
        m_lastObservedSwapNanoseconds = m_runStartNanoseconds - 1;
        m_refreshRate = PresentationPacing::validRefreshRate(
            m_window->screen() ? m_window->screen()->refreshRate() : 60.0);
        m_nextTargetNanoseconds = m_runStartNanoseconds;
        m_lastTickPresentationNanoseconds = m_runStartNanoseconds - std::llround(
            PresentationPacing::periodNanoseconds(m_targetFrameRate, m_refreshRate));
        scheduleNextFrame();
    }
}

void PresentationClock::handleFrameSwapped()
{
    const qint64 swap = m_swapTimestamp->load(std::memory_order_relaxed);
    if (!m_running || swap <= m_lastObservedSwapNanoseconds) {
        return;
    }
    m_lastObservedSwapNanoseconds = swap;
    const bool firstSwap = !m_phaseAnchorNanoseconds;
    m_lastSwapNanoseconds = swap;
    m_waitingForSwap = false;
    m_lastSwapCallbackNanoseconds = traceNow();
    updateRefreshRate(m_lastSwapCallbackNanoseconds);
    if (firstSwap) {
        // Bootstrap/fallback ticks have no display phase. Lock the ideal grid
        // to the first actual swap, then preserve that grid across later work.
        m_nextTargetNanoseconds = swap
            + PresentationPacing::periodNanoseconds(m_targetFrameRate, m_refreshRate);
        m_phaseAnchorNanoseconds = swap;
    }
    scheduleNextFrame();
}

void PresentationClock::presentNextFrame()
{
    if (!m_running) {
        return;
    }
    const qint64 now = traceNow();
    if (m_waitingForSwap) {
        // No completed frame by the next deadline: retain absolute timer pacing
        // on backends without swap feedback. A later swap restores vsync pacing.
        m_lastSwapNanoseconds.reset();
        m_phaseAnchorNanoseconds.reset();
        m_waitingForSwap = false;
        ++m_timerFallbacks;
    }
    updateRefreshRate(now);
    const auto next = PresentationPacing::schedule(
        now, m_nextTargetNanoseconds,
        m_targetFrameRate, m_refreshRate, m_phaseAnchorNanoseconds);
    if (next.wakeNanoseconds > now) {
        armTimer(next.wakeNanoseconds);
        return;
    }
    m_nextTargetNanoseconds = next.nextTargetNanoseconds;
    m_waitingForSwap = true;
    const qint64 wakeDeadline = m_wakeDeadlineNanoseconds;
    // Arm BEFORE synchronous frameTick work. This is the timer fallback and a
    // watchdog for missing swap feedback; a normal swap replaces it with the
    // next vsync-aligned wakeup.
    const qint64 watchdog = m_lastSwapNanoseconds
        ? std::llround(next.presentationNanoseconds
            + PresentationPacing::periodNanoseconds(m_targetFrameRate, m_refreshRate))
        : std::llround(m_nextTargetNanoseconds);
    armTimer(watchdog);
    if (m_trace) {
        std::lock_guard lock(m_trace->buffer->mutex);
        m_trace->pending.wakeDeadline = wakeDeadline;
        m_trace->pending.tickTimestamp = now;
        m_trace->pending.presentation = next.presentationNanoseconds;
        m_trace->pending.swapCallback = m_lastSwapCallbackNanoseconds;
        m_trace->pending.fallbacks = m_timerFallbacks;
    }
    tickAndRequestUpdate(next.presentationNanoseconds);
}

void PresentationClock::tickAndRequestUpdate(qint64 presentationNanoseconds)
{
    // Advance along the presentation timeline, including skipped slots. Actual
    // submissions do not move the phase origin; no GUI-work time is added to
    // the nominal period and stalls are not hidden by a clock-level clamp.
    const qreal deltaSeconds = (presentationNanoseconds - m_lastTickPresentationNanoseconds)
        / 1'000'000'000.0;
    if (deltaSeconds > 0.0) {
        m_lastTickPresentationNanoseconds = presentationNanoseconds;
        if (m_trace) {
            QObject *source = m_trace->simulationSource.data();
            const qreal before = source ? source->property("simulationTime").toDouble() : 0;
            const qreal step = source ? source->property("physicsStepSeconds").toDouble() : 0;
            const qint64 start = traceNow();
            Q_EMIT presentationTick(presentationNanoseconds);
            Q_EMIT frameTick(deltaSeconds);
            const qint64 duration = traceNow() - start;
            const qreal after = source ? source->property("simulationTime").toDouble() : 0;
            std::lock_guard lock(m_trace->buffer->mutex);
            m_trace->pending.tick += duration;
            ++m_trace->pending.ticks;
            if (step > 0) m_trace->pending.steps += std::max<qint64>(0, qRound64((after - before) / step));
        } else {
            Q_EMIT presentationTick(presentationNanoseconds);
            Q_EMIT frameTick(deltaSeconds);
        }
    }
    if (m_running) {
        if (m_trace) {
            std::lock_guard lock(m_trace->buffer->mutex);
            m_trace->pending.requestTimestamp = traceNow();
        }
        // QQuickWindow::update goes directly through Qt Quick's render loop,
        // including its vsync throttling, without QWindow's update-request delay.
        m_window->update();
    }
}

void PresentationClock::scheduleNextFrame()
{
    const qint64 now = traceNow();
    updateRefreshRate(now);
    const auto next = PresentationPacing::schedule(
        now, m_nextTargetNanoseconds,
        m_targetFrameRate, m_refreshRate, m_phaseAnchorNanoseconds);
    m_wakeDeadlineNanoseconds = next.wakeNanoseconds;
    if (next.wakeNanoseconds <= now) {
        // Full-rate rendering can start immediately after swap feedback. A
        // zero timer would add another event-loop turn before requesting it.
        m_wakeTimer.stop();
        presentNextFrame();
    } else {
        armTimer(next.wakeNanoseconds);
    }
}

void PresentationClock::updateRefreshRate(qint64 nowNanoseconds)
{
    const qreal refreshRate = PresentationPacing::validRefreshRate(
        m_window->screen() ? m_window->screen()->refreshRate() : 60.0);
    if (refreshRate != m_refreshRate) {
        m_refreshRate = refreshRate;
        m_phaseAnchorNanoseconds = m_lastSwapNanoseconds;
        m_nextTargetNanoseconds = m_lastSwapNanoseconds
            ? *m_lastSwapNanoseconds + PresentationPacing::periodNanoseconds(m_targetFrameRate, refreshRate)
            : nowNanoseconds;
    }
}

void PresentationClock::armTimer(qint64 deadlineNanoseconds)
{
    m_wakeDeadlineNanoseconds = deadlineNanoseconds;
    m_wakeTimer.setInterval(std::chrono::nanoseconds(
        std::max<qint64>(0, deadlineNanoseconds - traceNow())));
    m_wakeTimer.start();
}

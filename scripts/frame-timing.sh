#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
set -euo pipefail
exec python3 - "$@" <<'PY'
import argparse
import csv
import json
import math
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import tempfile
import time

# This code runs only beneath dbus-run-session with a scrubbed environment.
SESSION = r'''
import os, signal, subprocess, sys, time
from pathlib import Path
binary, directory, duration, outputs, width, height = sys.argv[1:]
directory = Path(directory)
children = []

def stop(process):
    if process.poll() is None:
        process.terminate()
        try: process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()

def interrupted(signum, frame):
    raise RuntimeError("Interrupted")
signal.signal(signal.SIGTERM, interrupted)
signal.signal(signal.SIGINT, interrupted)
try:
    # --virtual is mandatory. No DISPLAY or parent WAYLAND_DISPLAY is inherited.
    with (directory / "compositor.log").open("w") as log:
        compositor = subprocess.Popen([
            "kwin_wayland", "--virtual", "--width", width, "--height", height,
            "--output-count", outputs, "--socket", "pvs-frame-timing",
            "--no-lockscreen", "--no-global-shortcuts", "--no-kactivities"],
            stdout=log, stderr=subprocess.STDOUT)
        children.append(compositor)
        socket = Path(os.environ["XDG_RUNTIME_DIR"]) / "pvs-frame-timing"
        deadline = time.monotonic() + 30
        while not socket.is_socket():
            if compositor.poll() is not None or time.monotonic() > deadline:
                raise RuntimeError("Headless virtual KWin unavailable; see compositor.log")
            time.sleep(0.1)
        environment = dict(os.environ, WAYLAND_DISPLAY="pvs-frame-timing",
            QT_QPA_PLATFORM="wayland", QSG_INFO="1",
            PVS_FRAME_TRACE=str(directory / "frames.csv"),
            PVS_FRAME_TRACE_DURATION_MS=str(round(float(duration) * 1000)))
        with (directory / "app.log").open("w") as app_log:
            app = subprocess.Popen([binary, "--preview"], env=environment,
                stdout=app_log, stderr=subprocess.STDOUT)
            children.append(app)
            deadline = time.monotonic() + float(duration) + 30
            while app.poll() is None:
                if compositor.poll() is not None:
                    raise RuntimeError("Virtual compositor exited during measurement")
                if time.monotonic() > deadline:
                    raise RuntimeError("Timed preview failed to exit")
                time.sleep(0.2)
            if app.returncode:
                raise RuntimeError("Preview failed; see app.log")
finally:
    for process in reversed(children): stop(process)
'''


def percentiles(values):
    values = sorted(values)
    if not values:
        return None
    def percentile(p):
        index = (len(values) - 1) * p
        lower = math.floor(index)
        upper = math.ceil(index)
        return round(values[lower] + (values[upper] - values[lower]) * (index - lower), 4)
    return {"p50": percentile(.50), "p95": percentile(.95),
            "p99": percentile(.99), "max": round(values[-1], 4)}


def summarize(directory, outputs, duration, rate, warmup, graphics_api=None, check=False, size="3440x1440"):
    failures = []
    with (directory / "frames.csv").open() as stream:
        rows = list(csv.DictReader(stream))
    resources = [r for r in rows if r["kind"] == "resource"]
    frames = [r for r in rows if r["kind"] == "frame"]
    if not frames or len(resources) < 2:
        raise RuntimeError("Trace lacks frames or resource samples")
    first = min(int(r["timestamp_ns"]) for r in frames)
    frames = [r for r in frames if int(r["timestamp_ns"]) >= first + warmup * 1e9]
    windows = sorted(set(r["window"] for r in frames), key=int)
    if len(windows) != outputs:
        raise RuntimeError(f"Expected {outputs} output windows, got {len(windows)}")
    app_log = (directory / "app.log").read_text()
    if graphics_api and f"PVS graphics backend: {graphics_api}\n" not in app_log:
        failures.append(f"Requested {graphics_api} was not selected (including possible fallback)")
    renderer = re.findall(r".*(?:GL_RENDERER|GL_VENDOR|OpenGL VENDOR|graphics API|backend|Graphics API).*", app_log, re.I)
    if re.search(r"llvmpipe|softpipe|software rasterizer|software backend|Qt Quick software", app_log, re.I):
        rendering = "software"
    elif re.search(r"GL_RENDERER|OpenGL VENDOR|Vulkan device|D3D.*adapter|Metal device", app_log, re.I):
        rendering = "GPU (see renderer details)"
    else:
        rendering = "unknown; inspect app.log and compositor.log"
    start, end = resources[0], resources[-1]
    wall_ns = int(end["timestamp_ns"]) - int(start["timestamp_ns"])
    report = {
        "duration_requested_s": duration, "outputs": outputs, "output_size": size,
        "graphics_api_requested": graphics_api,
        "configured_fps": rate, "warmup_excluded_s": warmup, "rendering": rendering,
        "renderer_details": renderer, "window_details": [line for line in app_log.splitlines() if "Frame trace window" in line],
        "rss_mib": {"start": round(int(start["rss_bytes"]) / 2**20, 2),
                    "end": round(int(end["rss_bytes"]) / 2**20, 2),
                    "peak": round(max(int(r["rss_bytes"]) for r in resources) / 2**20, 2)},
        "cpu_percent_one_core": round(100 * (int(end["cpu_ns"]) - int(start["cpu_ns"])) / wall_ns, 2),
        "windows": {},
        "caveats": ["Virtual KWin output; frameSwapped is Qt swap/submission, not physical scanout or Wayland presentation feedback.",
                    "Sync includes scene-graph updatePaintNode; render is CPU command recording, not GPU elapsed time.",
                    "GUI time sums synchronous frameTick callbacks consumed by each scene-graph sync, including JS and native marshalling.",
                    "Simulation steps are measured from Snakes simulationTime / physicsStepSeconds; -1 means unavailable.",
                    "RSS peak is sampled once per second; CPU is process-wide, 100% equals one core; startup included.",
                    "In-memory CSV buffer contributes to RSS; trace writes on graceful exit.",
                    "Snakes caps configured auto at 60; swap feedback aligns pacing, with an absolute-timer fallback.",
                    "Submission cadence and phase do not establish compositor refresh. Advertised refresh comes from QScreen; compositor.log has no presentation timestamps.",
                    "Over-deadline counts measure lateness, not dropped frames. Skipped-slot estimates round to the nearest nominal interval; no scanout feedback is available.",
                    "When a sync consumes multiple ticks, pacing timestamps describe its last tick; timer_fallbacks is a cumulative per-clock counter."]}
    for window in windows:
        if graphics_api and not re.search(rf"Frame trace initialized window {window} graphics API {graphics_api}\b", app_log):
            failures.append(f"Window {window}: no initialized {graphics_api} scene graph")
        if check and graphics_api == "vulkan" and not re.search(
                rf"Frame trace initialized window {window} graphics API vulkan render thread true swap interval 0\b", app_log):
            failures.append(f"Window {window}: Vulkan requires a render thread and swap interval 0")
        selected = [r for r in frames if r["window"] == window]
        intervals = [int(r["interval_ns"]) / 1e6 for r in selected if int(r["interval_ns"]) > 0]
        if not intervals:
            raise RuntimeError(f"No frame intervals for window {window}")
        timestamps = [int(r["timestamp_ns"]) for r in selected]
        elapsed_ns = timestamps[-1] - timestamps[0]
        refresh_match = re.search(rf"Frame trace window {window} .*refresh Hz ([\d.]+)", app_log)
        refresh = float(refresh_match[1]) if refresh_match else None
        target = re.search(rf"Frame trace window {window} .*target FPS (\d+)", app_log)
        target_fps = int(target[1]) if target else None
        configured_target = 60 if rate == "auto" else int(rate)
        if check:
            if not refresh or not target_fps:
                failures.append(f"Window {window}: missing positive target/refresh rate")
            else:
                if target_fps != configured_target:
                    failures.append(f"Window {window}: target FPS {target_fps} != configured {configured_target}")
                expected_hz = refresh / max(1, math.ceil(refresh / configured_target))
                expected_frames = expected_hz * (duration - warmup)
                median = percentiles(intervals)["p50"]
                expected_interval = 1000 / expected_hz
                if len(selected) < .8 * expected_frames:
                    failures.append(f"Window {window}: {len(selected)} frames < 80% of {expected_frames:.1f} expected")
                if abs(median / expected_interval - 1) > .25:
                    failures.append(f"Window {window}: median {median:.4f}ms differs >25% from {expected_interval:.4f}ms")
        cadence = {
            "mean_interval_ms": round(sum(intervals) / len(intervals), 4),
            "observed_submission_hz": round((len(timestamps) - 1) * 1e9 / elapsed_ns, 4) if elapsed_ns > 0 else None,
            "advertised_refresh_hz": refresh,
            "interval_histogram_0.5ms": {},
        }
        for value in intervals:
            bucket = f"{math.floor(value * 2) / 2:.1f}"
            cadence["interval_histogram_0.5ms"][bucket] = cadence["interval_histogram_0.5ms"].get(bucket, 0) + 1
        cadence["interval_histogram_0.5ms"] = dict(sorted(cadence["interval_histogram_0.5ms"].items(), key=lambda pair: float(pair[0])))
        if refresh and refresh >= 1:
            refresh_ns = 1e9 / refresh
            cadence["submission_phase_ms"] = percentiles([
                ((timestamp - timestamps[0] + refresh_ns / 2) % refresh_ns - refresh_ns / 2) / 1e6
                for timestamp in timestamps])
        # Exact nominal deadlines, avoiding the bias of rounding 16.666... to 16.67.
        misses = {}
        for hz in (60, 30):
            deadline = 1e9 / hz
            missed = [int(r["interval_ns"]) for r in selected if int(r["interval_ns"]) > deadline]
            misses[f"{1000 / hz:.2f}_ms"] = {
                "intervals_over_deadline": len(missed),
                "intervals_over_deadline_plus_1ms": sum(v > deadline + 1e6 for v in missed),
                "estimated_skipped_slots_nearest_interval": sum(max(0, math.floor(v / deadline + .5) - 1) for v in missed)}
        ticks = [r for r in selected if int(r["tick_callbacks"]) > 0]
        report["windows"][window] = {
            "frames": len(selected), "frame_interval_ms": percentiles(intervals),
            "cadence": cadence,
            "missed_deadlines": misses,
            "gui_tick_ms": percentiles([int(r["gui_tick_ns"]) / 1e6 for r in ticks]),
            "sync_ms": percentiles([int(r["sync_ns"]) / 1e6 for r in selected]),
            "render_ms": percentiles([int(r["render_ns"]) / 1e6 for r in selected]),
            "simulation_steps": sum(max(0, int(r["simulation_steps"])) for r in selected),
            "tick_callbacks": sum(int(r["tick_callbacks"]) for r in selected)}
        if "tick_timestamp_ns" in selected[0]:
            paced = [r for r in ticks if int(r["tick_timestamp_ns"]) > 0 and int(r["request_timestamp_ns"]) > 0]
            def differences(end, start):
                return percentiles([(int(r[end]) - int(r[start])) / 1e6 for r in paced if int(r[start]) > 0])
            report["windows"][window]["pacing"] = {
                "timer_fallbacks_total": max(int(r["timer_fallbacks"]) for r in selected),
                "wake_lateness_ms": differences("tick_timestamp_ns", "wake_deadline_ns"),
                "request_to_swap_ms": differences("timestamp_ns", "request_timestamp_ns"),
                "swap_minus_prediction_ms": differences("timestamp_ns", "predicted_presentation_ns"),
                "swap_callback_to_tick_ms": differences("tick_timestamp_ns", "swap_callback_ns"),
            }
    report["validation"] = {"checked": check, "failures": failures}
    result = json.dumps(report, indent=2)
    (directory / "summary.json").write_text(result + "\n")
    print(f"\n{directory}\n{result}", flush=True)
    if failures:
        raise RuntimeError("Frame timing validation failed: " + "; ".join(failures))


parser = argparse.ArgumentParser(description="Isolated virtual-Wayland Snakes presentation baseline")
parser.add_argument("--binary", type=Path, default=Path("build-frametiming/bin/plasma-visual-screensaver"))
parser.add_argument("--duration", type=float, default=300, help="seconds per FPS/output combination (default 300)")
parser.add_argument("--outputs", type=int, nargs="+", default=[1, 2], choices=[1, 2, 3])
parser.add_argument("--rates", nargs="+", default=["30", "auto"], choices=["30", "60", "auto"])
parser.add_argument("--warmup", type=float, default=5, help="exclude initial seconds from frame percentiles")
parser.add_argument("--output-dir", type=Path, default=Path("frame-timing-results"))
parser.add_argument("--graphics-api", choices=["opengl", "vulkan"], help="force PVS backend; fail if it falls back")
parser.add_argument("--render-loop", choices=["basic", "threaded"], help="supply a Qt loop override (Vulkan should override basic)")
parser.add_argument("--check", action="store_true", help="require >=80%% expected frames per window and median within 25%% of target")
parser.add_argument("--width", type=int, default=3440)
parser.add_argument("--height", type=int, default=1440)
parser.add_argument("--summarize-only", action="store_true", help="reanalyze existing runs in --output-dir without launching processes")
args = parser.parse_args()
if not 0 <= args.warmup < args.duration or args.duration * 1000 > 2147483647:
    parser.error("duration must exceed nonnegative warmup and fit a Qt timer")
if args.width <= 0 or args.height <= 0:
    parser.error("output dimensions must be positive")
root = args.output_dir.resolve()
if args.summarize_only:
    for outputs in dict.fromkeys(args.outputs):
        for rate in dict.fromkeys(args.rates):
            directory = root / f"{outputs}output-{rate}fps"
            # Retain the original run's metadata when regenerating summaries.
            previous = json.loads((directory / "summary.json").read_text()) if (directory / "summary.json").exists() else {}
            summarize(directory, outputs, previous.get("duration_requested_s", args.duration), rate, args.warmup,
                args.graphics_api or previous.get("graphics_api_requested"), args.check,
                previous.get("output_size", f"{args.width}x{args.height}"))
    sys.exit(0)
binary = args.binary.resolve(strict=True)
root.mkdir(parents=True, exist_ok=True)
(root / "invocation.json").write_text(json.dumps({"argv": sys.argv[1:], "binary": str(binary)}, indent=2) + "\n")
for outputs in dict.fromkeys(args.outputs):
    for rate in dict.fromkeys(args.rates):
        directory = root / f"{outputs}output-{rate}fps"
        directory.mkdir(exist_ok=False)  # Never overwrite a previous measurement.
        config = directory / "config"
        config.mkdir()
        (config / "plasma-visual-screensaverrc").write_text(f"""[General]
VisualModule=snakes
AnimationDensity=100
TrailAmount=100
SnakeIntelligence=100
AnimationSpeed=100
AnimationScale=100
AnimationPalette=ocean
SnakeSelfCollisions=false
SnakeDeadlyWalls=true
ShowClock=false
ReducedMotion=false
FrameRate={0 if rate == 'auto' else int(rate)}
MonitorBehavior=independent
""")
        with tempfile.TemporaryDirectory(prefix="pvs-frame-timing-") as temporary:
            private = Path(temporary)
            for name in ("runtime", "home", "cache", "data", "tmp"):
                (private / name).mkdir(mode=0o700)
            environment = {
                "PATH": os.defpath + ":/usr/local/bin", "LANG": "C.UTF-8",
                "HOME": str(private / "home"), "TMPDIR": str(private / "tmp"),
                "XDG_RUNTIME_DIR": str(private / "runtime"), "XDG_CONFIG_HOME": str(config),
                "XDG_CONFIG_DIRS": str(config), "XDG_CACHE_HOME": str(private / "cache"),
                "XDG_DATA_HOME": str(private / "data"), "XDG_DATA_DIRS": "/usr/local/share:/usr/share",
                # Block system D-Bus too; the virtual compositor needs no real session services.
                "DBUS_SYSTEM_BUS_ADDRESS": "unix:path=" + str(private / "no-system-bus"),
                "QT_LOGGING_RULES": "qt.scenegraph.general=true;kwin_scene_opengl=true"}
            # Allow only explicit software-driver controls through the scrubbed
            # environment, never a desktop display/bus or Qt backend override.
            for name in ("VK_DRIVER_FILES", "VK_ICD_FILENAMES", "LIBGL_ALWAYS_SOFTWARE", "GALLIUM_DRIVER", "KWIN_COMPOSE"):
                if name in os.environ:
                    environment[name] = os.environ[name]
            if args.graphics_api:
                environment["PVS_GRAPHICS_API"] = args.graphics_api
            if args.render_loop:
                environment["QSG_RENDER_LOOP"] = args.render_loop
            bus_config = private / "bus.conf"
            bus_config.write_text(f"""<busconfig>
<type>session</type>
<listen>unix:tmpdir={private / 'runtime'}</listen>
<auth>EXTERNAL</auth>
<policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy>
</busconfig>
""")
            print(f"Running {outputs} virtual output(s), {rate} FPS, {args.duration:g}s", flush=True)
            session = subprocess.Popen(["dbus-run-session", "--config-file=" + str(bus_config), "--", sys.executable, "-c", SESSION,
                str(binary), str(directory), str(args.duration), str(outputs), str(args.width), str(args.height)],
                env=environment, start_new_session=True)
            def interrupt_session(signum, frame):
                # This process group was created by this script; no preexisting processes are signalled.
                if session.poll() is None:
                    os.killpg(session.pid, signal.SIGTERM)
                raise KeyboardInterrupt
            previous = {s: signal.signal(s, interrupt_session) for s in (signal.SIGINT, signal.SIGTERM)}
            try:
                try:
                    result = session.wait(timeout=args.duration + 65)
                except (subprocess.TimeoutExpired, KeyboardInterrupt):
                    if session.poll() is None:
                        os.killpg(session.pid, signal.SIGTERM)
                        try: session.wait(timeout=15)
                        except subprocess.TimeoutExpired:
                            os.killpg(session.pid, signal.SIGKILL)
                            session.wait()
                    raise
            finally:
                for sig, handler in previous.items(): signal.signal(sig, handler)
            if result:
                raise SystemExit(f"Isolated measurement blocked ({result}); inspect {directory}. No real-session fallback.")
        summarize(directory, outputs, args.duration, rate, args.warmup, args.graphics_api, args.check,
            f"{args.width}x{args.height}")
PY

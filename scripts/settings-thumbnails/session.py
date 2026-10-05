# SPDX-License-Identifier: GPL-3.0-or-later
# Runs on a private session bus: KWin + timed preview + Spectacle.
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time


def interrupted(signum, frame):
    raise SystemExit(128 + signum)


signal.signal(signal.SIGTERM, interrupted)
signal.signal(signal.SIGINT, interrupted)
binary, workdir, visual, out = sys.argv[1:5]
work = Path(workdir) / visual
cfgdir = work / "config"
cfgdir.mkdir(parents=True, exist_ok=True)
cfg = dict(VisualModule=visual, BackgroundStyle="black", AnimationPalette="ocean",
           ShowClock="false", FrameRate="30", MonitorBehavior="independent",
           AnimationDensity="50", AnimationScale="100", AnimationSpeed="100", TrailAmount="35")
for override in sys.argv[5:]:
    key, value = override.split("=", 1)
    cfg[key] = value
(cfgdir / "plasma-visual-screensaverrc").write_text(
    "[General]\n" + "".join(f"{key}={value}\n" for key, value in cfg.items()))
processes = []
# Short-lived socket directory only; captures/config/logs stay on disk.
with tempfile.TemporaryDirectory(prefix="pvs-thumb-") as runtime:
    env = dict(os.environ, XDG_CONFIG_HOME=str(cfgdir), XDG_RUNTIME_DIR=runtime)
    env.pop("WAYLAND_DISPLAY", None)
    env.pop("DISPLAY", None)
    try:
        with (work / "kwin.log").open("w") as log:
            kwin = subprocess.Popen([
                "kwin_wayland", "--virtual", "--width", os.environ.get("W", "640"),
                "--height", os.environ.get("H", "360"), "--socket", "pvs-thumb",
                "--no-lockscreen", "--no-global-shortcuts", "--no-kactivities"],
                env=dict(env, KWIN_SCREENSHOT_NO_PERMISSION_CHECKS="1"),
                stdout=log, stderr=subprocess.STDOUT)
        processes.append(kwin)
        deadline = time.monotonic() + 30
        while not (Path(runtime) / "pvs-thumb").is_socket():
            if kwin.poll() is not None or time.monotonic() > deadline:
                raise RuntimeError("KWin failed: " + (work / "kwin.log").read_text()[-2000:])
            time.sleep(0.1)
        appenv = dict(env, WAYLAND_DISPLAY="pvs-thumb", QT_QPA_PLATFORM="wayland",
                      PVS_FRAME_TRACE=str(work / "frames.csv"),
                      PVS_FRAME_TRACE_DURATION_MS=os.environ.get("DUR", "9000"))
        with (work / "app.log").open("w") as log:
            app = subprocess.Popen([binary, "--preview"], env=appenv,
                                   stdout=log, stderr=subprocess.STDOUT)
        processes.append(app)
        time.sleep(float(os.environ.get("GRAB_DELAY", "6")))
        if app.poll() is not None:
            raise RuntimeError("Preview exited: " + (work / "app.log").read_text()[-2000:])
        grab = subprocess.Popen(["spectacle", "-b", "-n", "-f", "-o", out],
                                env=dict(env, WAYLAND_DISPLAY="pvs-thumb", QT_QPA_PLATFORM="wayland"))
        processes.append(grab)
        if grab.wait(timeout=30) != 0 or not Path(out).is_file():
            raise RuntimeError("Spectacle did not produce a capture")
    finally:
        for process in reversed(processes):
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()

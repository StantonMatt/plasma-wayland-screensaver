#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Focused OpenGL RHI proof/captures on virtual KWin; never inherits a desktop socket/bus."""
import argparse
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time


def stop(process):
    if process and process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()


def worker(args):
    compositor = None
    try:
        with (args.output / 'compositor.log').open('w') as log:
            compositor = subprocess.Popen(['kwin_wayland', '--virtual', '--width', '4608', '--height', '2160',
                '--output-count', '1', '--socket', 'aurora-gpu', '--no-lockscreen', '--no-global-shortcuts', '--no-kactivities'],
                stdout=log, stderr=subprocess.STDOUT)
        deadline = time.monotonic() + 30
        while not (Path(os.environ['XDG_RUNTIME_DIR']) / 'aurora-gpu').is_socket():
            if compositor.poll() is not None or time.monotonic() > deadline:
                raise RuntimeError('Private virtual KWin unavailable; see compositor.log')
            time.sleep(.05)
        environment = dict(os.environ, WAYLAND_DISPLAY='aurora-gpu', QT_QPA_PLATFORM='wayland', QSG_RHI_BACKEND='opengl',
            QSG_INFO='1', VISUALS_REQUIRE_GPU='1', VISUALS_CAPTURE_DIR=str(args.output), QT_SCALE_FACTOR=str(args.scale))
        with (args.output / 'gpu-tests.log').open('w') as log:
            result = subprocess.run([str(args.build / 'bin' / args.test_binary)], env=environment,
                stdout=log, stderr=subprocess.STDOUT, timeout=180)
        print((args.output / 'gpu-tests.log').read_text())
        return result.returncode
    finally:
        stop(compositor)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--build', required=True, type=Path)
    p.add_argument('--output', required=True, type=Path)
    p.add_argument('--scale', type=float, default=1)
    p.add_argument('--test-binary', default='test-visualrenderer', choices=('test-visualrenderer', 'test-strokerenderer', 'test-bouncerenderer', 'test-matrixrenderer'))
    p.add_argument('--worker', action='store_true', help=argparse.SUPPRESS)
    args = p.parse_args()
    args.output = args.output.resolve()
    args.build = args.build.resolve()
    if args.worker:
        return worker(args)
    args.output.mkdir(parents=True, exist_ok=False)
    # Keep Wayland and bus socket paths short, even with a long artifact path.
    scratch = Path.home() / '.cache/agent-scratch'
    scratch.mkdir(exist_ok=True, parents=True)
    with tempfile.TemporaryDirectory(prefix='pvs-gpu-', dir=scratch) as directory:
        private = Path(directory)
        for name in ('runtime', 'home', 'config', 'cache', 'data', 'tmp'):
            (private / name).mkdir(mode=0o700)
        env = dict(PATH=os.defpath + ':/usr/local/bin', LANG='C.UTF-8', HOME=str(private / 'home'),
            TMPDIR=str(private / 'tmp'), XDG_RUNTIME_DIR=str(private / 'runtime'),
            XDG_CONFIG_HOME=str(private / 'config'), XDG_CONFIG_DIRS=str(private / 'config'),
            XDG_CACHE_HOME=str(private / 'cache'), XDG_DATA_HOME=str(private / 'data'),
            XDG_DATA_DIRS='/usr/local/share:/usr/share', DBUS_SYSTEM_BUS_ADDRESS='unix:path=' + str(private / 'no-system-bus'),
            QT_LOGGING_RULES='qt.scenegraph.general=true;kwin_scene_opengl=true')
        bus = private / 'bus.conf'
        bus.write_text(f'''<busconfig><type>session</type><listen>unix:tmpdir={private / 'runtime'}</listen>
<auth>EXTERNAL</auth><policy context="default"><allow send_destination="*"/><allow receive_sender="*"/>
<allow own="*"/></policy></busconfig>''')
        process = subprocess.Popen(['dbus-run-session', '--config-file=' + str(bus), '--', sys.executable,
            str(Path(__file__).resolve()), '--worker', '--build', str(args.build), '--output', str(args.output), '--scale', str(args.scale), '--test-binary', args.test_binary],
            env=env, start_new_session=True)
        def interrupted(signum, frame):
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGTERM)
            raise KeyboardInterrupt
        previous = {s: signal.signal(s, interrupted) for s in (signal.SIGINT, signal.SIGTERM)}
        try:
            return process.wait(timeout=240)
        finally:
            for s, handler in previous.items():
                signal.signal(s, handler)
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGTERM)
                try:
                    process.wait(timeout=15)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait()


if __name__ == '__main__':
    sys.exit(main())

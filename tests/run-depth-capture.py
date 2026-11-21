#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Deterministic contract fixture on three PRIVATE virtual-KWin outputs only."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'scripts'))
from visual_harness.layout import configure_l3
from visual_harness.isolation import SOCKET, bus_configuration, validated_environment


def stop(process):
    if process is not None and process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=10)


def worker(args):
    # --worker is callable directly: validation must precede even KWin startup.
    compositor_environment = validated_environment(os.environ, 'worker')
    compositor = None
    socket = SOCKET
    environment = dict(compositor_environment, WAYLAND_DISPLAY=socket, QT_QPA_PLATFORM='wayland',
                       QSG_RHI_BACKEND='opengl', QSG_INFO='1', VISUALS_REQUIRE_GPU='1')
    try:
        with (args.output / 'compositor.log').open('w') as log:
            compositor = subprocess.Popen(['kwin_wayland', '--virtual', '--width', '2560', '--height', '1440',
                '--output-count', '3', '--socket', socket, '--no-lockscreen', '--no-global-shortcuts', '--no-kactivities'],
                env=validated_environment(compositor_environment, 'worker'), stdout=log, stderr=subprocess.STDOUT)
        print(f'Private compositor PID {compositor.pid}', flush=True)
        deadline = time.monotonic() + 30
        while not (Path(environment['XDG_RUNTIME_DIR']) / socket).is_socket():
            if compositor.poll() is not None or time.monotonic() > deadline:
                raise RuntimeError('Private virtual KWin unavailable; see compositor.log')
            time.sleep(.05)
        configure_l3(environment, args.output)
        for scene, mode in [('spanning', 'seamless'), ('mirrored', 'synchronized'), ('independent', 'independent')]:
            if args.scene != 'all' and args.scene != scene:
                continue
            directory = args.output / 'scenes' / scene
            directory.mkdir(parents=True)
            request = dict(formatVersion=1, visual='fixture', scene=scene, seed=args.seed,
                           monitorMode=mode, captureTimesNs=args.times.split(','),
                           events=args.events)
            script = directory / 'request.json'
            script.write_text(json.dumps(request, indent=2))
            env = dict(environment, VISUALS_CAPTURE_DIR=str(directory), PVS_VISUAL_CAPTURE_SCRIPT=str(script))
            with (directory / 'gpu-tests.log').open('w') as log:
                test = subprocess.Popen([str(args.build / 'bin/test-visual-depth-capture')],
                                        env=validated_environment(env, 'client'),
                                        stdout=log, stderr=subprocess.STDOUT)
                print(f'{scene} fixture PID {test.pid}', flush=True)
                try:
                    code = test.wait(timeout=120)
                finally:
                    stop(test)
            if code:
                print((directory / 'gpu-tests.log').read_text())
                return code
            manifest = json.loads((directory / 'manifest.json').read_text())
            print(json.dumps(dict(scene=scene, firstFrameNs=manifest['firstFrameNs'], timings=manifest['timings'])), flush=True)
        return 0
    finally:
        stop(compositor)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--build', required=True, type=Path)
    parser.add_argument('--visual', default='fixture', choices=['fixture'])
    parser.add_argument('--scene', default='all', choices=['all', 'spanning', 'mirrored', 'independent'])
    parser.add_argument('--layout', default='l3', choices=['l3'])
    parser.add_argument('--round', type=int, default=1)
    parser.add_argument('--output', type=Path)
    parser.add_argument('--seed', default='1')
    parser.add_argument('--times', default='0,1000000000,1005000000,2000000000,3500000000', help='Comma-separated integer timeline nanoseconds')
    parser.add_argument('--event-script', type=Path, help='JSON array of fixture events (id,timelineNs,anchor,strength)')
    parser.add_argument('--worker', action='store_true', help=argparse.SUPPRESS)
    args = parser.parse_args()
    if not args.seed.isdecimal() or not 0 <= int(args.seed) < 2**64:
        parser.error('seed must be a decimal u64 string')
    if not all(t.isdecimal() and int(t) <= 3_600_000_000_000 for t in args.times.split(',')):
        parser.error('times must be integer nanoseconds in [0, 3600 seconds]')
    args.events = json.loads(args.event_script.read_text()) if args.event_script else [dict(id=1, timelineNs='1000000000', anchor=[2560, 600], strength=1)]
    if not isinstance(args.events, list):
        parser.error('event script must contain an array')
    for event in args.events:
        if (event.get('id') != 1 or not str(event.get('timelineNs', '')).isdecimal() or
                len(event.get('anchor', [])) != 2):
            parser.error('invalid fixture event')
    args.build = args.build.resolve()
    args.output = (args.output or Path.home() / f'.cache/agent-scratch/plasma-wayland-screensaver/3d-renders/{args.visual}/round-{args.round}').resolve()
    if args.worker:
        return worker(args)
    binary = args.build / 'bin/test-visual-depth-capture'
    if not binary.is_file():
        parser.error('Capture binary missing; configure with PVS_ENABLE_VISUAL_CAPTURE=ON')
    args.output.mkdir(parents=True, exist_ok=False)
    source_root = Path(__file__).resolve().parents[1]
    schema_hashes = {str(p.relative_to(source_root)): hashlib.sha256(p.read_bytes()).hexdigest()
                     for p in sorted((source_root / 'rust/snakes-core/src/visuals/depth').glob('*/mod.rs'))}
    scratch = Path.home() / '.cache/agent-scratch'
    scratch.mkdir(exist_ok=True, parents=True)
    with tempfile.TemporaryDirectory(prefix='pvs-depth-', dir=scratch) as directory:
        private = Path(directory)
        for name in ('runtime', 'home', 'config', 'cache', 'data', 'tmp'):
            (private / name).mkdir(mode=0o700)
        environment = dict(PATH=os.defpath + ':/usr/local/bin', LANG='C.UTF-8', HOME=str(private / 'home'),
            TMPDIR=str(private / 'tmp'), XDG_RUNTIME_DIR=str(private / 'runtime'),
            XDG_CONFIG_HOME=str(private / 'config'), XDG_CONFIG_DIRS=str(private / 'config'),
            XDG_CACHE_HOME=str(private / 'cache'), XDG_DATA_HOME=str(private / 'data'),
            XDG_DATA_DIRS='/usr/local/share:/usr/share', PVS_PRIVATE_HARNESS_ROOT=str(private),
            DBUS_SYSTEM_BUS_ADDRESS='unix:path=' + str(private / 'no-system-bus'),
            QT_LOGGING_RULES='qt.scenegraph.general=true;kwin_scene_opengl=true')
        bus = private / 'bus.conf'
        bus.write_text(bus_configuration(private))
        bus.chmod(0o600)
        environment = validated_environment(environment, 'bootstrap')
        head = subprocess.check_output(['git', '-C', str(source_root), 'rev-parse', 'HEAD'],
                                       env=environment, text=True, timeout=20).strip()
        diff = subprocess.check_output(['git', '-C', str(source_root), 'diff', 'HEAD'], env=environment, timeout=20)
        (args.output / 'invocation.json').write_text(json.dumps(dict(formatVersion=1, head=head, binary=str(binary),
            binarySha256=hashlib.sha256(binary.read_bytes()).hexdigest(), argv=sys.argv, layout=args.layout,
            schemaSourceHashes=schema_hashes, trackedDiffSha256=hashlib.sha256(diff).hexdigest(),
            shaderProvider='Qt built-in texture and vertex-colour materials; Qt version recorded in gpu-tests.log',
            scope='Contract fixture only; custom mixed-refresh modes, DPR and recordings deferred'), indent=2))
        process = subprocess.Popen(['dbus-run-session', '--config-file=' + str(bus), '--', sys.executable,
            str(Path(__file__).resolve()), '--worker', '--build', str(args.build), '--output', str(args.output),
            '--scene', args.scene, '--seed', args.seed, '--times', args.times, *(['--event-script', str(args.event_script.resolve())] if args.event_script else [])],
            env=validated_environment(environment, 'bootstrap'), start_new_session=True)
        print(f'Private capture process group {process.pid}', flush=True)
        def interrupted(signum, frame):
            raise KeyboardInterrupt
        previous = {s: signal.signal(s, interrupted) for s in (signal.SIGINT, signal.SIGTERM)}
        try:
            return process.wait(timeout=420)
        finally:
            for s, handler in previous.items():
                signal.signal(s, handler)
            # Descendants may survive an already exited dbus-run-session parent.
            try:
                os.killpg(process.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            stop(process)
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass


if __name__ == '__main__':
    sys.exit(main())

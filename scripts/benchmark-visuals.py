#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Process cost and Qt submission cadence for every PVS visual (Linux/Wayland)."""
import argparse
import configparser
import contextlib
import csv
import datetime
import hashlib
import json
import math
import os
from pathlib import Path
import re
import shlex
import shutil
import signal
import statistics
import subprocess
import sys
import tempfile
import time

VISUALS = ('none', 'aurora', 'bounce', 'constellation', 'fireflies',
           'kaleidoscope', 'matrix', 'orbs', 'ribbons', 'snakes', 'starfield')
ANIMATION = dict(AnimationDensity='55', AnimationScale='200', AnimationSpeed='300',
                 TrailAmount='100', AnimationPalette='ocean')
DEFAULTS = dict(ANIMATION, FrameRate='60', MonitorBehavior='seamless')
GENERAL_KEYS = set(DEFAULTS) | set(('BackgroundStyle BallCollisions BallCount BallElasticity BallGravity '
    'ClockMovement ClockSpeed CoverPanels IdleMinutes ReducedMotion ShowClock SnakeAggression SnakeDeadlyWalls '
    'SnakeIntelligence SnakeLengthLimit SnakePowerUps SnakeSelfCollisions SnakeStorePowerUps SnakeWorldEvents').split())
SIZES = [(2560, 1440), (3440, 1440), (1920, 1080)]
HERE = Path(__file__).resolve().parent
SCRATCH = Path.home() / '.cache/agent-scratch/plasma-wayland-screensaver/visual-bench'


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2, allow_nan=False) + '\n')


def command(argv, **kwargs):
    kwargs.setdefault('timeout', 30)
    try:
        return subprocess.run(argv, check=True, text=True, capture_output=True, **kwargs).stdout
    except subprocess.CalledProcessError as error:
        raise RuntimeError(f'{shlex.join(argv)} failed: {error.stderr.strip()}') from error


def stop(process, group=False):
    if process is None or (not group and process.poll() is not None):
        return
    def send(sig):
        try:
            if group:
                os.killpg(process.pid, sig)
            else:
                process.send_signal(sig)
        except ProcessLookupError:
            pass
    send(signal.SIGTERM)
    try:
        process.wait(timeout=2)
    except subprocess.TimeoutExpired:
        send(signal.SIGKILL)
        process.wait(timeout=5)
    finally:
        # dbus-run-session can exit before an orphaned app/compositor. Its own
        # process group remains ours even after the leader exits.
        if group:
            send(signal.SIGKILL)


def interrupted(signum, frame):
    raise InterruptedError(f'Interrupted by signal {signum}')


@contextlib.contextmanager
def cleanup_signals():
    """A second Ctrl-C must not interrupt desktop restoration."""
    previous = {s: signal.signal(s, signal.SIG_IGN) for s in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP)}
    try:
        yield
    finally:
        for sig, handler in previous.items():
            signal.signal(sig, handler)


def config_text(source, visual, overrides):
    config = configparser.ConfigParser(interpolation=None, strict=False)
    config.optionxform = str
    config.read_string(source)
    if not config.has_section('General'):
        config.add_section('General')
    settings = dict(DEFAULTS, **overrides)
    settings['VisualModule'] = visual
    config['General'].update(settings)
    # KConfig uses nested [Animations][module] groups. Saved profiles otherwise
    # override the General keys, including in a version-1 migrated user rc.
    for module in VISUALS:
        section = f'Animations][{module}'
        if not config.has_section(section):
            config.add_section(section)
        config[section].update({k: v for k, v in settings.items() if k in ANIMATION})
    import io
    result = io.StringIO()
    config.write(result, space_around_delimiters=False)
    return result.getvalue()


def percentile(values, fraction):
    if not values:
        return None
    values = sorted(values)
    index = (len(values) - 1) * fraction
    lo, hi = math.floor(index), math.ceil(index)
    return values[lo] + (values[hi] - values[lo]) * (index - lo)


def distribution(values):
    values = [x for x in values if x is not None]
    if not values:
        return None
    mean = statistics.mean(values)
    sd = statistics.stdev(values) if len(values) > 1 else None
    return dict(mean=mean, sd=sd, cv_percent=100 * sd / abs(mean) if sd is not None and mean else None,
                min=min(values), max=max(values), n=len(values))


def ratio(candidate, baseline):
    return candidate / baseline if candidate is not None and baseline is not None and baseline > 0 else None


def proc_sample(pid):
    # comm may contain spaces and parentheses; fields following its final ')'
    # start at stat field 3. utime/stime are fields 14/15, RSS is field 24.
    fields = Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()
    return dict(timestamp_ns=time.monotonic_ns(), ticks=int(fields[11]) + int(fields[12]),
                rss_mib=int(fields[21]) * os.sysconf('SC_PAGE_SIZE') / 2**20)


def trace_summary(path, app_log, start, end, stall_ms, expected_outputs):
    windows = {}
    for match in re.finditer(r'Frame trace window (\d+) (.*?) size QSize\((\d+), (\d+)\).*?target FPS (\d+).*?refresh Hz ([\d.]+)', app_log):
        windows[match[1]] = dict(name=match[2], size=[int(match[3]), int(match[4])],
                                target_fps=int(match[5]), refresh_hz=float(match[6]), rows=[])
    if len(windows) != expected_outputs:
        raise RuntimeError(f'Expected {expected_outputs} traced outputs, found {len(windows)}; inspect app.log')
    with path.open() as stream:
        for row in csv.DictReader(stream):
            if row['kind'] != 'frame':
                continue
            stamp = int(row['timestamp_ns'])
            if start <= stamp <= end:
                if row['window'] not in windows:
                    raise RuntimeError('Trace contains an unknown output')
                windows[row['window']]['rows'].append(row)
    for window in windows.values():
        rows = window.pop('rows')
        intervals = [int(r['interval_ns']) / 1e6 for r in rows
                     if int(r['interval_ns']) > 0 and int(r['timestamp_ns']) - int(r['interval_ns']) >= start]
        window.update(frames=len(rows), fps=len(rows) * 1e9 / (end - start),
                      interval_ms={f'p{p}': percentile(intervals, p / 100) for p in (50, 95, 99)},
                      stalls=sum(i > stall_ms for i in intervals))
    return windows


class NvidiaSamples:
    """Read nonblocking CSV with timestamps; totals are system GPU, not per-app."""
    def __init__(self, directory):
        self.process = None
        self.samples = []
        self.buffer = b''
        self.error = None
        self.directory = directory
        self.raw = (directory / 'gpu.csv').open('w')
        self.raw.write('timestamp_ns,index,uuid,utilization_percent,power_w\n')
        self.log = (directory / 'gpu.log').open('w')
        if shutil.which('nvidia-smi'):
            try:
                self.process = subprocess.Popen(['nvidia-smi', '--query-gpu=index,uuid,utilization.gpu,power.draw',
                    '--format=csv,noheader,nounits', '-lms', '500'], stdout=subprocess.PIPE, stderr=self.log)
                os.set_blocking(self.process.stdout.fileno(), False)
            except OSError as error:
                self.error = str(error)
        else:
            self.error = 'nvidia-smi unavailable'

    def poll(self):
        if self.process is None:
            return
        try:
            data = os.read(self.process.stdout.fileno(), 65536)
        except BlockingIOError:
            return
        self.buffer += data
        while b'\n' in self.buffer:
            line, self.buffer = self.buffer.split(b'\n', 1)
            now = time.monotonic_ns()
            columns = next(csv.reader([line.decode(errors='replace')]))
            if len(columns) != 4:
                continue
            index, uuid, utilization, power = [s.strip() for s in columns]
            def number(s):
                try:
                    value = float(s)
                    return value if math.isfinite(value) else None
                except ValueError:
                    return None
            sample = dict(timestamp_ns=now, index=index, uuid=uuid,
                          utilization_percent=number(utilization), power_w=number(power))
            self.samples.append(sample)
            self.raw.write(f'{now},{index},{uuid},{utilization},{power}\n')

    def close(self):
        self.poll()
        stop(self.process)
        if self.process:
            self.process.stdout.close()
        self.raw.close()
        self.log.close()

    def summary(self):
        devices = {}
        for uuid in sorted({r['uuid'] for r in self.samples}):
            rows = [r for r in self.samples if r['uuid'] == uuid]
            devices[uuid] = {k: distribution([r[k] for r in rows]) for k in ('utilization_percent', 'power_w')}
        return dict(devices=devices, unavailable=self.error or (None if devices else 'No usable NVIDIA samples; see gpu.log'))


def configure_outputs(environment, directory):
    before = json.loads(command(['kscreen-doctor', '--json'], env=environment))
    outputs = before['outputs']
    if len(outputs) != 3:
        raise RuntimeError('Virtual compositor did not expose three outputs')
    # Add modes first: selecting a newly added mode in the same atomic request
    # is not supported by KScreen. Nothing here can reach the real Wayland socket.
    additions = []
    for output, (width, height) in zip(outputs, SIZES):
        if not any(m['size'] == dict(width=width, height=height) for m in output['modes']):
            additions.append(f"output.{output['id']}.addCustomMode.{width}.{height}.60000.reduced")
    if additions:
        command(['kscreen-doctor', *additions], env=environment)
    settings = []
    x = 0
    for output, (width, height) in zip(outputs, SIZES):
        prefix = f"output.{output['id']}"
        settings += [prefix + f'.mode.{width}x{height}@60', prefix + f'.position.{x},0', prefix + '.scale.1']
        x += width
    command(['kscreen-doctor', *settings], env=environment)
    actual = json.loads(command(['kscreen-doctor', '--json'], env=environment))
    write_json(directory / 'outputs.json', actual)
    x = 0
    for output, (width, height) in zip(actual['outputs'], SIZES):
        mode = next(m for m in output['modes'] if m['id'] == output['currentModeId'])
        if mode['size'] != dict(width=width, height=height) or output['pos'] != dict(x=x, y=0) or output['scale'] != 1:
            raise RuntimeError('Virtual output geometry differs from requested layout; see outputs.json')
        x += width


def measure(job, entry, environment, compositor):
    if hashlib.sha256(Path(entry['binary']).read_bytes()).hexdigest() != job['binaries'][entry['label']]['sha256']:
        raise RuntimeError(f"Binary changed during benchmark: {entry['binary']}")
    directory = Path(entry['directory'])
    directory.mkdir()
    config = directory / 'config'
    config.mkdir()
    (config / 'plasma-visual-screensaverrc').write_text(config_text(job['source_config'], entry['visual'], job['overrides']))
    duration, warmup = job['duration'], job['warmup']
    env = dict(environment, XDG_CONFIG_HOME=str(config), XDG_CONFIG_DIRS=str(config),
               PVS_FRAME_TRACE=str(directory / 'frames.csv'),
               # Allow time for graceful trace flush after the external window.
               PVS_FRAME_TRACE_DURATION_MS=str(math.ceil((warmup + duration + 2) * 1000)), QSG_INFO='1')
    if job['graphics_api']:
        env['PVS_GRAPHICS_API'] = job['graphics_api']
    app = gpu = None
    samples = []
    try:
        with (directory / 'app.log').open('w') as log:
            app = subprocess.Popen([entry['binary'], '--preview'], env=env, stdout=log, stderr=subprocess.STDOUT)
        def alive():
            if app.poll() is not None:
                raise RuntimeError(f'Preview exited early ({app.returncode}); inspect {directory}/app.log')
            if compositor and compositor.poll() is not None:
                raise RuntimeError('Virtual KWin exited during measurement')
        deadline = time.monotonic() + warmup
        while time.monotonic() < deadline:
            alive()
            time.sleep(min(.05, max(0, deadline - time.monotonic())))
        alive()
        gpu = NvidiaSamples(directory)
        first = proc_sample(app.pid)
        samples.append(first)
        deadline = first['timestamp_ns'] / 1e9 + duration
        next_sample = time.monotonic() + .25
        while time.monotonic() < deadline:
            alive()
            gpu.poll()
            now = time.monotonic()
            if now >= next_sample:
                samples.append(proc_sample(app.pid))
                next_sample = now + .25
            time.sleep(min(.025, max(0, deadline - time.monotonic())))
        last = proc_sample(app.pid)
        samples.append(last)
        gpu.close()
        gpu_closed = gpu
        gpu = None
        # Let the app's own timer quit normally: SIGTERM does not flush Qt CSV.
        end_wait = time.monotonic() + 15
        while app.poll() is None:
            if time.monotonic() > end_wait:
                raise RuntimeError('Timed preview failed to exit; no complete trace')
            time.sleep(.05)
        if app.returncode:
            raise RuntimeError(f'Preview failed ({app.returncode}); inspect app.log')
        with (directory / 'resources.csv').open('w') as stream:
            writer = csv.DictWriter(stream, fieldnames=['timestamp_ns', 'ticks', 'rss_mib'])
            writer.writeheader()
            writer.writerows(samples)
        log = (directory / 'app.log').read_text()
        if job['graphics_api'] and f"PVS graphics backend: {job['graphics_api']}\n" not in log:
            raise RuntimeError('Requested graphics backend was not selected; inspect app.log')
        if job['target'] == 'virtual':
            software = bool(re.search(r'llvmpipe|softpipe|lavapipe|software rasterizer|software backend', log, re.I))
            if software != (job['virtual_renderer'] == 'software'):
                raise RuntimeError('Observed renderer does not match --virtual-renderer; inspect app.log')
        windows = trace_summary(directory / 'frames.csv', log, first['timestamp_ns'], last['timestamp_ns'], job['stall_ms'], 3 if job['target'] == 'virtual' else job['real_outputs'])
        actual_config = configparser.ConfigParser(interpolation=None)
        actual_config.read(config / 'plasma-visual-screensaverrc')
        reduced = actual_config.getboolean('General', 'ReducedMotion', fallback=False)
        # None with no clock, or a centred clock, may legitimately submit almost no frames.
        static_floor = entry['visual'] == 'none' and (
            not actual_config.getboolean('General', 'ShowClock', fallback=True)
            or actual_config.get('General', 'ClockMovement', fallback='bounce') == 'center')
        if not reduced and not static_floor and any(w['frames'] < 2 for w in windows.values()):
            raise RuntimeError('An animated output submitted fewer than two frames during measurement')
        if job['target'] == 'virtual' and sorted(w['size'] for w in windows.values()) != sorted(map(list, SIZES)):
            raise RuntimeError('Traced window sizes do not match virtual output layout')
        result = dict(entry, measurement_start_ns=first['timestamp_ns'], measurement_end_ns=last['timestamp_ns'],
                      measured_seconds=(last['timestamp_ns'] - first['timestamp_ns']) / 1e9,
                      cpu_percent=100 * (last['ticks'] - first['ticks']) / os.sysconf('SC_CLK_TCK') /
                          ((last['timestamp_ns'] - first['timestamp_ns']) / 1e9),
                      rss_mib=dict(start=first['rss_mib'], end=last['rss_mib'], peak=max(s['rss_mib'] for s in samples)),
                      windows=windows, gpu=gpu_closed.summary(),
                      renderer_details=[line for line in log.splitlines() if re.search(r'GL_RENDERER|GL_VENDOR|OpenGL VENDOR|graphics API|graphics backend|Vulkan device', line, re.I)])
        # A silent missing animation is not a cheap successful benchmark.
        errors = re.findall(r'.*(?:failed to load|is not a type|ReferenceError|TypeError|QQmlApplicationEngine failed|Cannot assign).*', log, re.I)
        if errors:
            raise RuntimeError('QML errors in preview: ' + '; '.join(errors[:3]))
        write_json(directory / 'result.json', result)
        print(f"  CPU {result['cpu_percent']:.1f}%  RSS peak {result['rss_mib']['peak']:.1f} MiB", flush=True)
        return result
    finally:
        with cleanup_signals():
            stop(app)
            if gpu:
                gpu.close()


def worker(job_path):
    job = json.loads(job_path.read_text())
    root = Path(job['root'])
    compositor = None
    environment = dict(os.environ)
    try:
        if job['target'] == 'virtual':
            with (root / 'compositor.log').open('w') as log:
                compositor = subprocess.Popen(['kwin_wayland', '--virtual', '--width', '3440', '--height', '1440',
                    '--output-count', '3', '--socket', 'pvs-visual-bench', '--no-lockscreen',
                    '--no-global-shortcuts', '--no-kactivities'], stdout=log, stderr=subprocess.STDOUT)
            socket = Path(environment['XDG_RUNTIME_DIR']) / 'pvs-visual-bench'
            deadline = time.monotonic() + 30
            while not socket.is_socket():
                if compositor.poll() is not None or time.monotonic() > deadline:
                    raise RuntimeError('Private virtual KWin unavailable; see compositor.log (no real fallback)')
                time.sleep(.1)
            environment.update(WAYLAND_DISPLAY='pvs-visual-bench', QT_QPA_PLATFORM='wayland')
            configure_outputs(environment, root)
        else:
            # Query on the real socket but the benchmark's private bus/config.
            outputs = json.loads(command(['kscreen-doctor', '--json'], env=environment))
            write_json(root / 'outputs.json', outputs)
            job['real_outputs'] = sum(o['enabled'] and o['connected'] for o in outputs['outputs'])
            if job['real_outputs'] < 1:
                raise RuntimeError('No active real outputs')
        for index, entry in enumerate(job['entries'], 1):
            print(f"[{index}/{len(job['entries'])}] {entry['visual']} {entry['label']} block {entry['block']}", flush=True)
            measure(job, entry, environment, compositor)
    finally:
        with cleanup_signals():
            stop(compositor)


class IdleGuard:
    def __init__(self, executable, milliseconds, directory):
        self.log = (directory / 'idle.log').open('w')
        self.process = subprocess.Popen([str(executable), str(milliseconds)], stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, stderr=self.log)
        os.set_blocking(self.process.stdout.fileno(), False)
        self.buffer = b''
        self.ready = self.idle = self.inhibited = False
        self.running = False
        self.heartbeat = time.monotonic()

    def events(self):
        try:
            data = os.read(self.process.stdout.fileno(), 65536)
        except BlockingIOError:
            data = b''
        self.buffer += data
        events = []
        while b'\n' in self.buffer:
            line, self.buffer = self.buffer.split(b'\n', 1)
            event = line.decode()
            events.append(event)
            self.log.write(f'{time.monotonic():.6f} {event}\n')
            self.log.flush()
            self.heartbeat = time.monotonic()
            if event == 'READY':
                self.ready = True
            elif event == 'IDLE':
                self.idle = True
            elif event == 'INHIBITED':
                self.inhibited = True
            elif event == 'RESUMED':
                self.idle = False
                if self.running:
                    raise InterruptedError('User input detected; aborted preview and restoring daemon')
        if self.process.poll() is not None or time.monotonic() - self.heartbeat > 3:
            raise RuntimeError('Idle guard failed or heartbeat lost; refusing unguarded preview; see idle.log')
        return events

    def wait(self):
        startup = time.monotonic() + 10
        while not (self.ready and self.idle):
            self.events()
            if not self.ready and time.monotonic() > startup:
                raise RuntimeError('Idle protocol unavailable; see idle.log')
            time.sleep(.025)
        self.running = True
        self.events()  # Catch input queued at the idle/run boundary.

    def inhibit(self):
        self.process.stdin.write(b'inhibit\n')
        self.process.stdin.flush()
        deadline = time.monotonic() + 10
        while not self.inhibited:
            self.events()
            if time.monotonic() > deadline:
                raise RuntimeError('Idle/power inhibition failed; see idle.log')
            time.sleep(.025)

    def close(self):
        # EOF gives the helper a chance to release its cookies explicitly.
        self.process.stdin.close()
        try:
            self.process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            stop(self.process)
        self.process.stdout.close()
        self.log.close()


class DaemonPause:
    """Stop/start only the installed daemon's verified original systemd unit."""
    def __init__(self, directory):
        self.directory = directory
        self.unit = None
        self.original_pid = None
        self.restore_needed = False

    def discover(self):
        installed = Path('/usr/bin/plasma-visual-screensaver').resolve()
        matches = []
        for path in Path('/proc').iterdir():
            if not path.name.isdigit():
                continue
            try:
                executable = Path(os.readlink(path / 'exe').removesuffix(' (deleted)')).resolve()
                if path.stat().st_uid != os.getuid() or executable != installed:
                    continue
                argv = (path / 'cmdline').read_bytes().split(b'\0')
                if b'--background' in argv:
                    matches.append(path)
            except (FileNotFoundError, PermissionError, ProcessLookupError):
                continue
        if len(matches) > 1:
            raise RuntimeError('Multiple installed background daemons; refusing to stop ambiguous processes')
        if not matches:
            write_json(self.directory / 'daemon.json', dict(state='not running; will not start one'))
            return
        path = matches[0]
        group = (path / 'cgroup').read_text().strip().split('/')[-1]
        if not group.endswith('.service'):
            raise RuntimeError('Background daemon is not a restorable systemd user service; no desktop changes made')
        props = dict(line.split('=', 1) for line in command(['systemctl', '--user', 'show', group,
            '-p', 'MainPID', '-p', 'ActiveState', '-p', 'FragmentPath']).splitlines())
        if props['MainPID'] != path.name or props['ActiveState'] != 'active' or not props['FragmentPath']:
            raise RuntimeError('Daemon unit ownership/state mismatch; no desktop changes made')
        self.unit = group
        self.original_pid = int(path.name)
        write_json(self.directory / 'daemon.json', dict(unit=group, original_pid=int(path.name), properties=props))

    def pause(self):
        if self.unit:
            props = dict(line.split('=', 1) for line in command(['systemctl', '--user', 'show', self.unit,
                '-p', 'MainPID', '-p', 'ActiveState']).splitlines())
            if props.get('MainPID') != str(self.original_pid) or props.get('ActiveState') != 'active':
                raise RuntimeError('Daemon changed since discovery; refusing to change its new state')
            # Set before calling stop: interruption or a timeout must still restore.
            self.restore_needed = True
            command(['systemctl', '--user', 'stop', self.unit])
            if command(['systemctl', '--user', 'show', self.unit, '-p', 'ActiveState', '--value']).strip() != 'inactive':
                raise RuntimeError('Background daemon did not stop')

    def restore(self):
        if self.restore_needed:
            last_error = None
            for attempt in range(3):
                try:
                    command(['systemctl', '--user', 'start', self.unit], timeout=10)
                    state = command(['systemctl', '--user', 'show', self.unit, '-p', 'ActiveState', '--value'], timeout=10).strip()
                    if state != 'active':
                        raise RuntimeError(f'Unit state is {state}')
                    self.restore_needed = False
                    print(f'Restored background daemon: {self.unit}', flush=True)
                    return
                except (OSError, RuntimeError, subprocess.SubprocessError) as error:
                    last_error = error
                    if attempt < 2:
                        time.sleep(.5)
            raise RuntimeError(f'Failed to restore {self.unit}: {last_error}; run systemctl --user start {self.unit}')


def build_idle_helper(root):
    executable = root / 'idle-guard'
    flags = shlex.split(command(['pkg-config', '--cflags', '--libs', 'Qt6Core', 'Qt6DBus', 'wayland-client']))
    command(['c++', '-std=c++17', '-O2', '-fPIC', str(HERE / 'benchmark-visuals-idle.cpp'), '-o', str(executable), *flags])
    return executable


def session_environment(private, target, renderer, runtime):
    for name in ('home', 'runtime', 'cache', 'data', 'state', 'tmp', 'config'):
        (private / name).mkdir(mode=0o700)
    env = dict(PATH=os.defpath + ':/usr/local/bin', LANG='C.UTF-8', HOME=str(private / 'home'),
               TMPDIR=str(private / 'tmp'), XDG_RUNTIME_DIR=str(runtime),
               XDG_CONFIG_HOME=str(private / 'config'), XDG_CONFIG_DIRS=str(private / 'config'),
               XDG_CACHE_HOME=str(private / 'cache'), XDG_DATA_HOME=str(private / 'data'),
               XDG_STATE_HOME=str(private / 'state'), XDG_DATA_DIRS='/usr/local/share:/usr/share',
               DBUS_SYSTEM_BUS_ADDRESS='unix:path=' + str(private / 'no-system-bus'),
               QT_QPA_PLATFORM='wayland', QT_LOGGING_RULES='qt.scenegraph.general=true')
    if target == 'virtual':
        if renderer == 'software':
            env.update(LIBGL_ALWAYS_SOFTWARE='1', GALLIUM_DRIVER='llvmpipe', KWIN_COMPOSE='Q')
            # LIBGL_ALWAYS_SOFTWARE does not control Vulkan. Match CI's explicit
            # lavapipe ICD isolation; a missing ICD must fail, never use a GPU.
            icds = sorted(Path('/usr/share/vulkan/icd.d').glob('lvp_icd*.json'))
            env['VK_DRIVER_FILES'] = str(icds[0] if icds else private / 'no-software-vulkan-icd.json')
    else:
        # Keep the user's Wayland connection, never their D-Bus or config.
        for key in ('WAYLAND_DISPLAY', 'XDG_RUNTIME_DIR'):
            if not os.environ.get(key):
                raise RuntimeError(f'{key} must identify the real Plasma Wayland session')
            env[key] = os.environ[key]
    return env


def run_session(job, root, args):
    private = root / 'private'
    private.mkdir(mode=0o700)
    # Unix socket paths must fit 108 bytes, even when --output-dir is long.
    SCRATCH.mkdir(parents=True, exist_ok=True)
    runtime = Path(tempfile.mkdtemp(prefix='r-', dir=SCRATCH))
    guard = daemon = process = None
    try:
        env = session_environment(private, args.target, args.virtual_renderer, runtime)
        write_json(root / 'environment.json', env)
        # No activation dirs, system-bus connection, parent display or parent bus.
        bus = private / 'bus.conf'
        bus.write_text(f'''<busconfig><type>session</type><listen>unix:path={runtime / 'bus'}</listen>
<auth>EXTERNAL</auth><policy context="default"><allow send_destination="*"/>
<allow receive_sender="*"/><allow own="*"/></policy></busconfig>\n''')
        if args.target == 'real':
            daemon = DaemonPause(root)
            guard = IdleGuard(build_idle_helper(root), max(1, round(args.wait_idle * 60000)), root)
            print(f'Waiting for a fresh {args.wait_idle:g} minute input-idle interval (Ctrl-C cancels).', flush=True)
            guard.wait()
            daemon.discover()
            guard.inhibit()
            daemon.pause()
            guard.events()
        process = subprocess.Popen(['dbus-run-session', '--config-file=' + str(bus), '--', sys.executable,
                                    str(HERE / 'benchmark-visuals.py'), '--worker', str(root / 'job.json')],
                                   env=env, start_new_session=True)
        while process.poll() is None:
            if guard:
                guard.events()
            time.sleep(.025)
        if guard:
            guard.events()
        if process.returncode:
            raise RuntimeError(f'Private measurement session failed ({process.returncode}); inspect {root}')
    finally:
        with cleanup_signals():
            try:
                stop(process, group=True)
            finally:
                try:
                    if daemon:
                        daemon.restore()
                finally:
                    try:
                        if guard:
                            guard.close()
                    finally:
                        shutil.rmtree(private)
                        shutil.rmtree(runtime)


def scalar_metrics(run):
    windows = run['windows'].values()
    gpu = run['gpu']['devices'].values()
    powers = [d['power_w']['mean'] for d in gpu if d['power_w']]
    utilization = [d['utilization_percent']['mean'] for d in run['gpu']['devices'].values() if d['utilization_percent']]
    return dict(cpu_percent=run['cpu_percent'], rss_peak_mib=run['rss_mib']['peak'],
                min_output_fps=min(w['fps'] for w in windows),
                worst_p95_ms=max((w['interval_ms']['p95'] for w in run['windows'].values() if w['interval_ms']['p95'] is not None), default=None),
                stalls=sum(w['stalls'] for w in run['windows'].values()),
                gpu_power_w=sum(powers) if powers else None,
                gpu_utilization_percent=max(utilization) if utilization else None)


def validate_output_identity(runs):
    fields = ('name', 'size', 'target_fps', 'refresh_hz')
    reference = {key: {field: value.get(field) for field in fields} for key, value in runs[0]['windows'].items()}
    for run in runs[1:]:
        current = {key: {field: value.get(field) for field in fields} for key, value in run['windows'].items()}
        if current != reference:
            raise RuntimeError('Output identity, geometry or pacing settings changed between runs; cannot compare')


def summarize(job, root, args):
    runs = [json.loads((Path(e['directory']) / 'result.json').read_text()) for e in job['entries']]
    rows, failures = [], []
    for visual in job['visuals']:
        validate_output_identity([run for run in runs if run['visual'] == visual])
        row = dict(visual=visual, labels={})
        for label in dict.fromkeys(e['label'] for e in job['entries']):
            selected = [r for r in runs if r['visual'] == visual and r['label'] == label]
            metrics = [scalar_metrics(r) for r in selected]
            row['labels'][label] = dict(metrics={key: distribution([m[key] for m in metrics]) for key in metrics[0]},
                outputs={window: dict(fps=distribution([r['windows'][window]['fps'] for r in selected]),
                    **{f'p{p}_ms': distribution([r['windows'][window]['interval_ms'][f'p{p}'] for r in selected]) for p in (50, 95, 99)},
                    stalls=distribution([r['windows'][window]['stalls'] for r in selected])) for window in selected[0]['windows']})
        if 'candidate' in row['labels']:
            a, b = row['labels']['baseline'], row['labels']['candidate']
            row['candidate_baseline_ratios'] = {key: ratio(b['metrics'][key]['mean'] if b['metrics'][key] else None,
                a['metrics'][key]['mean'] if a['metrics'][key] else None) for key in a['metrics']}
            row['output_ratios'] = {window: {key: ratio(b['outputs'][window][key]['mean'] if b['outputs'][window][key] else None,
                a['outputs'][window][key]['mean'] if a['outputs'][window][key] else None) for key in a['outputs'][window]}
                for window in a['outputs']}
            block_ratios = []
            for block in range(1, args.repeats + 1):
                grouped = {label: [r['cpu_percent'] for r in runs if r['visual'] == visual and r['block'] == block and r['label'] == label]
                           for label in ('baseline', 'candidate')}
                value = ratio(statistics.mean(grouped['candidate']), statistics.mean(grouped['baseline']))
                if value is not None:
                    block_ratios.append(value)
            row['cpu_block_ratios'] = block_ratios
            row['cpu_block_ratio_distribution'] = distribution(block_ratios)
            # Pooled within-label coefficient of variation is a noise estimate,
            # not a confidence interval or a significance claim.
            cvs = [r['metrics']['cpu_percent']['cv_percent'] for r in (a, b)]
            row['cpu_noise_cv_percent'] = math.sqrt(statistics.mean(c * c for c in cvs)) if all(c is not None for c in cvs) else None
            for key, threshold, lower in [('cpu_percent', args.max_cpu_regression, False),
                                           ('min_output_fps', args.max_frame_loss, True),
                                           ('worst_p95_ms', args.max_p95_regression, False)]:
                value = row['candidate_baseline_ratios'][key]
                if threshold is not None and value is None and key == 'cpu_percent' \
                        and row['labels']['candidate']['metrics'][key]['mean'] > 0:
                    failures.append(f'{visual}: {key} baseline is zero but candidate is not; cannot pass the gate')
                if threshold is not None and value is not None and (value < 1 - threshold if lower else value > 1 + threshold):
                    failures.append(f'{visual}: {key} candidate/baseline {value:.4f} violates {threshold:.1%} limit')
            # Gate each output as well, so a healthy output cannot hide a stall.
            for window, ratios in row['output_ratios'].items():
                for key, threshold, lower in [('fps', args.max_frame_loss, True), ('p95_ms', args.max_p95_regression, False)]:
                    value = ratios[key]
                    if threshold is not None and value is not None and (value < 1 - threshold if lower else value > 1 + threshold):
                        failures.append(f'{visual} output {window}: {key} ratio {value:.4f} violates {threshold:.1%} limit')
        rows.append(row)
    rank_label = 'baseline'
    rows.sort(key=lambda r: r['labels'][rank_label]['metrics']['cpu_percent']['mean'], reverse=True)
    report = dict(schema_version=1, invocation=job, ranked_by='baseline mean process CPU, descending',
        visuals=rows, runs=runs, failures=failures, caveats=[
            'CPU 100% equals one core; compositor CPU is excluded. RSS peak is sampled every 250ms; trace buffers count toward RSS.',
            'Qt frameSwapped measures submission, not physical scanout or GPU duration. FPS uses the entire external measurement window, including stalls.',
            f'Stall means an inter-submission interval > {job["stall_ms"]:g}ms; static outputs can legitimately submit no frames.',
            'Virtual software rendering ranks a CPU raster workload; it does not predict hardware GPU cost. Virtual hardware is still not physical scanout.',
            'NVIDIA samples describe all system GPU activity; software runs do not attribute that activity to the benchmark.',
            'CV is observed repeat variation, not a confidence interval. Random animation/snake state, clocks, load, temperatures and power states cause noise.',
            'Settings and config SHA are recorded. ShowClock and snake/ball settings are inherited unless overridden. None includes the inherited clock.',
            'Runs restart the animation; use longer windows for mature snake ecosystems and compare the same target, driver and settings.'])
    write_json(root / 'summary.json', report)
    text = ['| Visual | CPU % | CPU CV % | RSS MiB | FPS min | p95 worst ms | Stalls | GPU % | GPU W | C/B CPU | Noise CV % |',
            '|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|']
    def fmt(value, digits=2):
        return f'{value:.{digits}f}' if value is not None else '—'
    for row in rows:
        for label, result in row['labels'].items():
            metrics = result['metrics']
            def mean(key):
                return metrics[key]['mean'] if metrics[key] else None
            title = row['visual'] + (f' ({label})' if len(row['labels']) > 1 else '')
            values = [title, fmt(mean('cpu_percent')), fmt(metrics['cpu_percent']['cv_percent']),
                fmt(mean('rss_peak_mib')), fmt(mean('min_output_fps')), fmt(mean('worst_p95_ms')), fmt(mean('stalls'), 1),
                fmt(mean('gpu_utilization_percent')), fmt(mean('gpu_power_w')),
                fmt(row.get('candidate_baseline_ratios', {}).get('cpu_percent'), 4) if label == 'candidate' else '—',
                fmt(row.get('cpu_noise_cv_percent')) if label == 'candidate' else '—']
            text.append('| ' + ' | '.join(values) + ' |')
    text += ['', 'Ranked by baseline process CPU (highest cost first). FPS is minimum across outputs; full per-output p50/p95/p99 and C/B ratios are in JSON.',
             'GPU metrics are system-wide; software virtual runs do not measure application GPU cost.', '',
             ('Gate: FAIL\n' + '\n'.join(failures)) if failures else ('Gate: PASS' if any(t is not None for t in (args.max_cpu_regression, args.max_frame_loss, args.max_p95_regression)) else 'Report only; no regression thresholds requested.'), '']
    markdown = '\n'.join(text)
    (root / 'summary.md').write_text(markdown)
    print('\n' + markdown + f'Artifacts: {root}', flush=True)
    return 1 if failures else 0


def parser():
    p = argparse.ArgumentParser(prog='benchmark-visuals.sh', description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter, epilog='''
Default workload: all 11 visuals (None floor), density 55, scale 200, speed 300,
trails 100, ocean palette, 60 FPS, seamless monitors. Snake/ball/clock settings
come from the source rc [General]. Each run copies the rc into a private config
and sets both General and per-visual animation profiles; the source is never edited.

Single binary: one run per visual per repeat. A/B: --binary is baseline,
--candidate is candidate; each repeat is an ABBA block per visual (four runs).
CPU ratios use means; noise is pooled within-label CPU CV. JSON includes each
run, per-output FPS/p50/p95/p99/stalls, GPU devices, settings and binary hashes.
Ranked Markdown + JSON are written under --output-dir. Nonzero exit on invalid
traces, failed previews, input/cancellation, or an explicitly requested gate.

Real target requires Plasma Wayland ext-idle-notify v2, c++, pkg-config and
Qt6Core/Qt6DBus/wayland-client development files for a small no-window guard.
--wait-idle waits a fresh uninterrupted interval from guard creation (even if
already idle). Input notifications ignore inhibitors; input aborts within the
25ms supervisor polling interval plus process teardown. Guard failure fails closed.
The installed daemon's verified systemd user unit is stopped only after idle,
then restarted exactly via that unit on success, Ctrl-C, TERM, HUP or error.
Unmanaged/ambiguous daemons are refused. A stopped daemon is never started.
The guard holds screen-lock/display-power inhibitors only during the run and
releases them on exit. No daemon or real output is changed in virtual mode.

Virtual target: private bus, HOME/config/cache/runtime, three 60Hz outputs at
2560x1440+0+0, 3440x1440+2560+0, 1920x1080+6000+0. Software GL by default;
hardware is opt-in and still measures submissions, not physical presentation.
NVIDIA utilisation/power are optional system-wide samples every 500ms.
SIGKILL/power loss cannot run traps; do not use them for cancellation.

Examples (run heavy finite benchmarks through ~/.local/bin/heavy):
  scripts/benchmark-visuals.sh --target virtual --duration 20 --warmup 5
  scripts/benchmark-visuals.sh --binary /usr/bin/plasma-visual-screensaver \\
    --candidate build/bin/plasma-visual-screensaver --visuals snakes aurora \\
    --repeats 3 --set ShowClock=false --max-cpu-regression .10 --max-frame-loss .05
  scripts/benchmark-visuals.sh --target real --wait-idle 5 --duration 60
''')
    p.add_argument('--target', choices=['real', 'virtual'], default='virtual')
    p.add_argument('--binary', type=Path, default=Path('/usr/bin/plasma-visual-screensaver'), help='baseline executable (default installed binary)')
    p.add_argument('--candidate', '--candidate-binary', type=Path, help='candidate executable; enables interleaved ABBA')
    p.add_argument('--visuals', nargs='+', choices=VISUALS, default=list(VISUALS), help='selected visuals (default all 11)')
    p.add_argument('--duration', type=float, default=60, help='measured seconds per run, AFTER warmup (default 60)')
    p.add_argument('--warmup', type=float, default=10, help='unmeasured seconds per run (default 10)')
    p.add_argument('--repeats', type=int, default=1, help='single runs or full ABBA blocks per visual (default 1)')
    p.add_argument('--set', dest='settings', action='append', default=[], metavar='KEY=VALUE', help='override a General key; repeatable (animation profiles updated too)')
    p.add_argument('--config', type=Path, default=Path(os.environ.get('XDG_CONFIG_HOME', str(Path.home() / '.config'))) / 'plasma-visual-screensaverrc', help='source rc to copy (default user rc; missing file uses application defaults)')
    p.add_argument('--output-dir', type=Path, help='new, nonexisting artifact directory (default timestamped on-disk agent scratch)')
    p.add_argument('--wait-idle', type=float, default=1, metavar='MINUTES', help='real only: fresh input-idle interval (default 1; 0 arms after 1ms)')
    p.add_argument('--virtual-renderer', choices=['software', 'hardware'], default='software')
    p.add_argument('--graphics-api', choices=['opengl', 'vulkan'], help='force app graphics API and reject fallback')
    p.add_argument('--stall-ms', type=float, default=50, help='frame interval counted as a stall when above this (default 50ms)')
    p.add_argument('--max-cpu-regression', type=float, metavar='FRACTION', help='optional A/B gate, e.g. .10 allows 10%% more CPU')
    p.add_argument('--max-frame-loss', type=float, metavar='FRACTION', help='optional A/B gate per-output FPS, e.g. .05 allows 5%% loss')
    p.add_argument('--max-p95-regression', type=float, metavar='FRACTION', help='optional A/B gate per-output p95 interval')
    return p


def main(argv=None):
    p = parser()
    args = p.parse_args(argv)
    for key in ('duration', 'warmup', 'wait_idle', 'stall_ms'):
        value = getattr(args, key)
        if not math.isfinite(value) or value < 0 or (key in ('duration', 'stall_ms') and value == 0):
            p.error(f'{key} must be finite and positive (warmup/wait-idle may be zero)')
    if (args.duration + args.warmup + 2) * 1000 > 2147483647 or args.wait_idle * 60000 > 2147483647 or args.repeats < 1:
        p.error('timer duration exceeds limit or repeats is less than one')
    thresholds = (args.max_cpu_regression, args.max_frame_loss, args.max_p95_regression)
    if any(t is not None and (not math.isfinite(t) or not 0 <= t < 1) for t in thresholds):
        p.error('gate fractions must be in [0,1)')
    if any(t is not None for t in thresholds) and not args.candidate:
        p.error('relative gates require --candidate')
    overrides = {}
    for setting in args.settings:
        if '=' not in setting:
            p.error('--set requires KEY=VALUE')
        key, value = setting.split('=', 1)
        if key not in GENERAL_KEYS or '\n' in value or '\r' in value:
            p.error('unknown General key or invalid value; choose visuals with --visuals')
        overrides[key] = value
    binaries = {'baseline': args.binary.resolve(strict=True)}
    if args.candidate:
        binaries['candidate'] = args.candidate.resolve(strict=True)
    for binary in binaries.values():
        if not os.access(binary, os.X_OK):
            p.error(f'binary is not executable: {binary}')
    root = args.output_dir or SCRATCH / datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%S.%fZ')
    root = root.resolve()
    root.mkdir(parents=True, mode=0o700, exist_ok=False)
    source = args.config.read_text() if args.config.exists() else ''
    job = dict(root=str(root), target=args.target, virtual_renderer=args.virtual_renderer,
        duration=args.duration, warmup=args.warmup, visuals=list(dict.fromkeys(args.visuals)), repeats=args.repeats,
        source_config=source, source_config_path=str(args.config.resolve()), source_config_sha256=hashlib.sha256(source.encode()).hexdigest(),
        overrides=overrides, default_settings=DEFAULTS, graphics_api=args.graphics_api, stall_ms=args.stall_ms,
        argv=sys.argv[1:], host=dict(uname=list(os.uname()), cpu_count=os.cpu_count()),
        binaries={label: dict(path=str(binary), sha256=hashlib.sha256(binary.read_bytes()).hexdigest()) for label, binary in binaries.items()}, entries=[])
    for visual in job['visuals']:
        for block in range(1, args.repeats + 1):
            labels = ['baseline', 'candidate', 'candidate', 'baseline'] if args.candidate else ['baseline']
            for position, label in enumerate(labels, 1):
                job['entries'].append(dict(visual=visual, block=block, position=position, label=label,
                    binary=str(binaries[label]), directory=str(root / f'{visual}-{block}-{position}-{label}')))
    write_json(root / 'job.json', job)
    print(f'Artifacts: {root}\nTarget {args.target}; {len(job["entries"])} runs, each {args.warmup:g}s warm + {args.duration:g}s measured.', flush=True)
    try:
        run_session(job, root, args)
        result = summarize(job, root, args)
        write_json(root / 'status.json', dict(status='failed-gate' if result else 'complete'))
        return result
    except BaseException as error:
        write_json(root / 'status.json', dict(status='aborted' if isinstance(error, (InterruptedError, KeyboardInterrupt)) else 'failed', error=str(error)))
        raise


if __name__ == '__main__':
    for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
        signal.signal(sig, interrupted)
    try:
        if len(sys.argv) == 3 and sys.argv[1] == '--worker':
            worker(Path(sys.argv[2]))
        else:
            sys.exit(main())
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        print(f'Benchmark stopped: {error}', file=sys.stderr)
        sys.exit(130 if isinstance(error, InterruptedError) else 1)

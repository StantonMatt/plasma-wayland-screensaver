#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Capture registered visual scenes or the contract fixture on private virtual KWin."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import signal
import statistics
import subprocess
import sys
import tempfile
import time
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'scripts'))
from visual_harness.layout import configure_layout, L3
from visual_harness.isolation import SOCKET, bus_configuration, validated_environment


def stop(process):
    if process is not None and process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=10)


LAYOUTS = {'l3': L3, 'single': L3[:1], 'wide': [dict(L3[1], x=0)],
           'pair': [L3[0], dict(L3[2], x=2560, y=0)],
           'hd': [dict(L3[2], x=0, y=0)], 'fullhd': [dict(L3[2], x=0, y=0)],
           'wide-pair': [dict(L3[1], x=0), dict(L3[2], x=3440, y=0)]}


def scene_catalogs(directory):
    return {path.parent.name: path for path in directory.glob('*/round.json')}


def worker(args):
    # Refuse inherited desktop access before reading the request or creating any files.
    environment = validated_environment(os.environ, 'worker')
    if args.visual == 'fixture':
        return capture(args, environment, L3, args.output, None)
    if args.scene == 'benchmark':
        return capture(args, environment, L3, args.output, {'benchmark': True})
    catalog = args.catalog
    scenes = catalog["scenes"]
    if args.scene == 'activation-study':
        names = ['legacy', 'activation', 'activation', 'legacy'] * (5 if args.warm_activation else 3)
        pairs = [scene_request(args, name) for name in names]
        return capture(args, environment, pairs[0]['outputs'], args.output, dict(pairs=pairs))
    requested = list(scenes) if args.scene == 'all' else args.scene.split(',')
    for name in requested:
        spec = scene_request(args, name)
        if args.clip:
            if 'video' not in spec:
                raise ValueError('--clip requires a video scene')
            start, frames = args.clip
            spec['video'] = dict(spec['video'], start=start, frames=frames)
            spec['captureTimesNs'] = [str(start * 1_000_000_000)]
        directory = args.output / 'scenes' / name
        directory.mkdir(parents=True)
        result = capture(args, environment, spec['outputs'], directory, spec)
        if result:
            return result
    return 0


def scene_request(args, name):
    spec = args.catalog['scenes'][name]
    request = dict(spec, formatVersion=1, visual=args.visual, scene=name, outputs=LAYOUTS[spec['layout']],
                   settings={**args.catalog['defaults'], **spec.get('settings', {})},
                   events=args.events if args.event_script else spec.get('events', []))
    if spec.get('legacy') or spec.get('softwareProbe'):
        request['events'] = []
    if 'prewarmSeconds' in args.catalog:
        request.setdefault('prewarmSeconds', args.catalog['prewarmSeconds'])
    return request


def validate_events(events, ids):
    if not isinstance(events, list):
        raise ValueError('event script must contain an array')
    for event in events:
        if not isinstance(event, dict) or type(event.get('id')) is not int or event['id'] not in ids:
            raise ValueError(f'unknown event ID (declared IDs: {sorted(ids)})')
        timestamp = str(event.get('timelineNs', ''))
        anchor = event.get('anchor')
        strength = event.get('strength', 1)
        if (not timestamp.isdecimal() or int(timestamp) > 2**63 - 1
                or not isinstance(anchor, list) or len(anchor) != 2
                or any(type(n) not in (float, int) or not math.isfinite(n) for n in [*anchor, strength])):
            raise ValueError('invalid event timestamp, anchor or strength')
        event['timelineNs'] = timestamp


def run_scene(args, environment, target, product):
    script = target / 'request.json'
    script.write_text(json.dumps(product, indent=2))
    env = dict(environment, VISUALS_CAPTURE_DIR=str(target), PVS_VISUAL_CAPTURE_SCRIPT=str(script))
    test_args = [str(args.build / 'bin/test-visual-depth-capture'), 'productScene']
    if product.get('gpuTiming'):
        test_args.extend(['-maxwarnings', '0'])
    with (target / 'gpu-tests.log').open('w') as log:
        test = subprocess.Popen(test_args,
                                env=validated_environment(env,'client'), stdout=log, stderr=subprocess.STDOUT)
        print(f"{product['scene']} capture PID {test.pid}", flush=True)
        try:
            code = test.wait(timeout=3600 if product.get('video') else 300)
        finally:
            stop(test)
    if not code and product.get('video'):
        finish_video(target, product)
    if code:
        print((target / 'gpu-tests.log').read_text()[-4000:])
    return code


def finish_video(directory, scene):
    from PIL import Image, ImageOps
    video = scene['video']
    name = video.get('name', 'world')
    films = [name, 'seam'] if video.get('seam') else [name]
    report = directory / 'manifest.json'
    manifest = json.loads(report.read_text())
    manifest['recordings'] = []
    for film in films:
        path = directory / (film + '.mp4')
        if path.exists():
            if video.get('reviewCopy'):
                lossless = directory / 'lossless' / path.name
                lossless.parent.mkdir(exist_ok=True)
                path.rename(lossless)
                subprocess.run(['ffmpeg', '-loglevel', 'error', '-i', str(lossless), '-threads', '4',
                                '-c:v', 'libx264', '-crf', '16', '-preset', 'veryfast',
                                '-pix_fmt', 'yuv420p', '-movflags', '+faststart', str(path)],
                               check=True, timeout=1800)
                manifest.setdefault('reviewRecordings', []).append(path.name)
                path = lossless
            manifest['recordings'].append(str(path.relative_to(directory)))
        else:
            # Capture frames use five-digit indices; contact sheets share the
            # film prefix but must never become part of the recorded sequence.
            sequence = film + '-' + '[0-9]' * 5 + '.png'
            images = sorted(directory.glob(sequence))
            if len(images) != video['frames']:
                raise RuntimeError('Incomplete frame sequence')
            sheet = Image.new('RGB',(960,540))
            for i in range(9):
                with Image.open(images[(len(images)-1)*i//8]) as frame:
                    sheet.paste(ImageOps.contain(frame,(320,180)),((i%3)*320,(i//3)*180))
            sheet.save(directory / (film + '-contact.png'))
            manifest['recordings'].append(dict(sequence=sequence, contact=film+'-contact.png'))
    if video.get('exposureStrip'):
        # Eight exposures of the same bright detail in the lower third of the frame.
        still = directory / manifest['frames'][0]['panorama']
        with Image.open(still) as im:
            lum = im.convert('L')
            # The primary recording accompanies a native seam crop at half panorama size.
            if video.get('seam'):
                lum = lum.resize((lum.width // 2, lum.height // 2), Image.Resampling.BILINEAR)
            width, height = lum.size
            region = (64, 2*height//3, width-64, height-64)
            lum = lum.crop(region)
            y,x = divmod(lum.tobytes().index(lum.getextrema()[1]),lum.width)
            x,y=int(x+region[0]-48),int(y+region[1]-48)
        out = directory / video['exposureStrip']
        stride = max(1, video['frames'] // 8)
        if (directory / (name + '.mp4')).exists():
            subprocess.run(['ffmpeg','-loglevel','error','-i',str(directory / (name + '.mp4')),
               '-vf',f"select='not(mod(n,{stride}))',crop=96:96:{x}:{y},tile=8x1",'-frames:v','1',
               '-threads','1',str(out)],check=True, timeout=60)
        else:
            strip=Image.new('RGB',(768,96))
            for i in range(8):
                with Image.open(directory / f'{name}-{stride*i:05d}.png') as im:
                    strip.paste(im.crop((x,y,x+96,y+96)),(96*i,0))
            strip.save(out)
        manifest['strip']=out.name
    report.write_text(json.dumps(manifest,indent=2))


def capture(args, compositor_environment, roles, target, product):
    compositor = None
    socket = SOCKET
    environment = dict(compositor_environment, WAYLAND_DISPLAY=socket, QT_QPA_PLATFORM='wayland',
                       QSG_RHI_BACKEND='opengl', QSG_INFO='1', VISUALS_REQUIRE_GPU='1')
    try:
        with (target / 'compositor.log').open('w') as log:
            compositor = subprocess.Popen(['kwin_wayland', '--virtual', '--width', '2560', '--height', '1440',
                '--output-count', str(len(roles)), '--socket', socket, '--no-lockscreen', '--no-global-shortcuts', '--no-kactivities'],
                env=validated_environment(compositor_environment, 'worker'), stdout=log, stderr=subprocess.STDOUT)
        print(f'Private compositor PID {compositor.pid}', flush=True)
        deadline = time.monotonic() + 30
        while not (Path(environment['XDG_RUNTIME_DIR']) / socket).is_socket():
            if compositor.poll() is not None or time.monotonic() > deadline:
                raise RuntimeError('Private virtual KWin unavailable; see compositor.log')
            time.sleep(.05)
        configure_layout(environment, target, roles)
        if product and product.get('benchmark'):
            script = Path(__file__).resolve().parents[1] / 'scripts/benchmark-visuals.sh'
            for mode in ('independent','seamless'):
                argv = [str(script), '--target', 'virtual', '--private-depth-session', '--virtual-renderer', 'hardware',
                        '--binary', str(args.build / 'bin/plasma-visual-screensaver'), '--visuals', 'none', args.visual,
                        *(part for value in args.benchmark_set for part in ('--set', value)),
                        '--set', 'MonitorBehavior='+mode, '--set', 'FrameRate=60', '--set', 'ShowClock=true',
                        '--set', 'ClockMovement=bounce', '--duration', '30', '--warmup', '5',
                        '--output-dir', str(target / mode)]
                with (target / (mode+'.log')).open('w') as log:
                    result = subprocess.run(argv, env=validated_environment(environment,'client'), stdout=log,
                                            stderr=subprocess.STDOUT, timeout=300)
                if result.returncode:
                    print((target / (mode+'.log')).read_text()[-3000:])
                    return result.returncode
            return 0
        if product and 'pairs' in product:
            samples = {'legacy': [], 'activation': []}
            cold = {}
            cache_files = []
            if args.warm_activation:
                # The worker owns one private HOME and XDG cache for the entire
                # compositor session. Prime both paths before timing the warm pairs.
                for request in product['pairs'][:2]:
                    name = request['scene']
                    child = target / ('warmup-' + name)
                    child.mkdir()
                    if run_scene(args, environment, child, request):
                        return 1
                    cold[name] = json.loads((child / 'manifest.json').read_text())['activationNs']
                for label in ('XDG_CACHE_HOME', 'HOME'):
                    directory = Path(environment[label])
                    cache_files.extend(f'{label}/{p.relative_to(directory)}'
                                       for p in directory.rglob('*') if p.is_file()
                                       and any('cache' in s.lower() for s in p.relative_to(directory).parts))
            for i, request in enumerate(product['pairs']):
                child = target / ('%02d-%s' % (i, request['scene']))
                child.mkdir()
                if run_scene(args, environment, child, request):
                    return 1
                ns = json.loads((child / 'manifest.json').read_text())['activationNs']
                samples[request['scene']].append(ns)
            medians = {key: statistics.median(times) for key, times in samples.items()}
            delta = medians['activation'] - medians['legacy']
            paired = [new - old for new, old in zip(samples['activation'], samples['legacy'])]
            report = dict(samplesNs=samples, mediansNs=medians, addedNs=delta,
                          pairedDeltasNs=paired, maxPairedDeltaNs=max(paired))
            if args.warm_activation:
                median = statistics.median(paired)
                p90 = sorted(paired)[math.ceil(0.9 * len(paired)) - 1]
                report.update(discardedColdNs=cold, privateWarmCacheFiles=sorted(cache_files),
                              medianPairedDeltaNs=median, p90PairedDeltaNs=p90,
                              medianLimitNs=25_000_000, maxLimitNs=100_000_000,
                              passed=len(paired) >= 10 and median <= 25_000_000 and max(paired) <= 100_000_000)
            else:
                report.update(limitNs=50_000_000, passed=max(paired) <= 50_000_000)
            (target / 'activation.json').write_text(json.dumps(report, indent=2) + '\n')
            return 0 if report['passed'] else 1
        if product:
            return run_scene(args, environment, target, product)
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
                test = subprocess.Popen([str(args.build / 'bin/test-visual-depth-capture'), 'fixture'],
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
    catalogs = scene_catalogs(Path(__file__).with_name('depth') / 'scenes')
    parser.add_argument('--visual', default='fixture', choices=sorted(catalogs))
    parser.add_argument('--scene', default='all')
    parser.add_argument('--layout', default='l3', choices=['l3'])
    parser.add_argument('--clip', type=int, nargs=2, metavar=('START_SECONDS', 'FRAMES'),
                        help='Capture a shorter diagnostic interval from one scripted video scene')
    parser.add_argument('--benchmark-set', action='append', default=[], metavar='KEY=VALUE',
                        help='Use a benchmark-visuals.sh General setting for the private CPU study')
    parser.add_argument('--warm-activation', action='store_true',
                        help='Warm both renderers in one private driver/Qt cache before ten interleaved activation pairs')
    parser.add_argument('--round', type=int, default=1)
    parser.add_argument('--output', type=Path)
    parser.add_argument('--seed', default='1')
    parser.add_argument('--times', default='0,1000000000,1005000000,2000000000,3500000000', help='Comma-separated integer timeline nanoseconds')
    parser.add_argument('--event-script', type=Path, help='JSON array of visual events (id,timelineNs,anchor,strength)')
    parser.add_argument('--worker', action='store_true', help=argparse.SUPPRESS)
    args = parser.parse_args()
    if args.warm_activation and args.scene != 'activation-study':
        parser.error('--warm-activation requires --scene activation-study')
    if args.benchmark_set and args.scene != 'benchmark':
        parser.error('--benchmark-set is only valid for the benchmark scene')
    if args.clip and (args.visual == 'fixture' or args.scene == 'all' or ',' in args.scene or
                      not 0 <= args.clip[0] <= 3600 or not 60 <= args.clip[1] <= 3600):
        parser.error('--clip needs one video scene, a start in [0, 3600] and 60 to 3600 frames')
    if not args.seed.isdecimal() or not 0 <= int(args.seed) < 2**64:
        parser.error('seed must be a decimal u64 string')
    if not all(t.isdecimal() and int(t) <= 3_600_000_000_000 for t in args.times.split(',')):
        parser.error('times must be integer nanoseconds in [0, 3600 seconds]')
    try:
        args.catalog = json.loads(catalogs[args.visual].read_text())
        ids = args.catalog['eventIds']
        if (not isinstance(ids, list) or any(type(i) is not int or not 0 <= i <= 0xffffffff for i in ids)
                or len(set(ids)) != len(ids)):
            raise ValueError('eventIds must list distinct unsigned 32-bit IDs')
        args.events = (json.loads(args.event_script.read_text()) if args.event_script
                       else args.catalog.get('defaultEvents', []))
        validate_events(args.events, ids)
        if args.visual != 'fixture':
            scenes = args.catalog['scenes']
            selected = list(scenes) if args.scene == 'all' else args.scene.split(',')
            if args.scene == 'activation-study':
                selected = ['legacy', 'activation']
            if args.scene == 'benchmark':
                selected = []
            for name in selected:
                scene = scenes[name]
                validate_events(scene.get('events', []), ids)
                video = scene.get('video')
                if video and video.get('exposureStrip') and video['frames'] < 8:
                    raise ValueError('an exposure strip requires at least eight frames')
            if args.scene == 'activation-study' and scenes['legacy']['layout'] != scenes['activation']['layout']:
                raise ValueError('paired activation needs matching layouts')
        elif args.scene not in ('all', 'spanning', 'mirrored', 'independent'):
            raise ValueError(f'unknown fixture scene {args.scene!r}')
    except (ValueError, KeyError, OSError) as error:
        parser.error(str(error))
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
        new_paths = subprocess.check_output(['git', '-C', str(source_root), 'ls-files', '--others',
                                            '--exclude-standard', '-z', '--', 'rust/snakes-core/src/visuals/depth',
                                            'rust/snakes-core/include', 'src/visualdepthregistrations.cpp.in', 'src/visuals/common',
                                            f'src/visuals/{args.visual}',
                                            f'src/shaders/depth/{args.visual}', f'tests/depth/{args.visual}', f'tests/depth/scenes/{args.visual}',
                                            'tests/depth/productcapture.inc', 'tests/depth/luminance.cpp',
                                            'tests/check-depth-luminance.py'], env=environment, timeout=20)
        untracked_hashes = {}
        for raw in new_paths.split(b'\0'):
            if raw:
                relative = os.fsdecode(raw)
                file = source_root / relative
                if file.is_file():
                    untracked_hashes[relative] = hashlib.sha256(file.read_bytes()).hexdigest()
        (args.output / 'invocation.json').write_text(json.dumps(dict(formatVersion=1, head=head, binary=str(binary),
            binarySha256=hashlib.sha256(binary.read_bytes()).hexdigest(), argv=sys.argv, layout=args.layout,
            schemaSourceHashes=schema_hashes, untrackedSourceHashes=untracked_hashes, trackedDiffSha256=hashlib.sha256(diff).hexdigest(),
            shaderProvider='Qt RHI; see scene log for observed backend',
            scope='Private virtual KWin, 60 Hz outputs, DPR 1'), indent=2))
        process = subprocess.Popen(['dbus-run-session', '--config-file=' + str(bus), '--', sys.executable,
            str(Path(__file__).resolve()), '--worker', '--build', str(args.build), '--output', str(args.output),
            '--visual', args.visual, '--scene', args.scene, '--seed', args.seed, '--times', args.times,
            *(['--warm-activation'] if args.warm_activation else []),
            *(part for value in args.benchmark_set for part in ('--benchmark-set', value)), *(['--clip', *map(str, args.clip)] if args.clip else []), *(['--event-script', str(args.event_script.resolve())] if args.event_script else [])],
            env=validated_environment(environment, 'bootstrap'), start_new_session=True)
        print(f'Private capture process group {process.pid}', flush=True)
        def interrupted(signum, frame):
            raise KeyboardInterrupt
        previous = {s: signal.signal(s, interrupted) for s in (signal.SIGINT, signal.SIGTERM)}
        try:
            code = process.wait(timeout=7200 if args.visual != 'fixture' else 420)
            if not code:
                manifests = sorted(args.output.glob('scenes/*/manifest.json'))
                result = dict(visual=args.visual, scenes=[str(p.relative_to(args.output)) for p in manifests])
                if args.scene == 'activation-study':
                    result['activation'] = 'activation.json'
                if args.scene == 'benchmark':
                    result['benchmarks'] = [f'{mode}/summary.json' for mode in ('independent', 'seamless')]
                sequences = [p for p in manifests if json.loads(p.read_text()).get('videoFrames', 0)]
                if sequences:
                    report = args.output / 'luminance.json'
                    checker = Path(__file__).with_name('check-depth-luminance.py')
                    code = subprocess.run([sys.executable, str(checker), '--build', str(args.build),
                                           '--report', str(report), *map(str,sequences)], timeout=3600).returncode
                    result['luminance'] = dict(report=report.name, passed=not code)
                (args.output / 'manifest.json').write_text(json.dumps(result, indent=2)+'\n')
            return code
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

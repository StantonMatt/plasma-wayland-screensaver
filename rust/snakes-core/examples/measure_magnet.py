#!/usr/bin/env python3
"""Run the 12-case MAGNET-only matrix in an isolated, temporary crate.

Invoke through heavy. Production spawns avoid consecutive equal kinds; the
isolated fixture permits repeats because its spawn set contains only MAGNET.
No source in the working tree is modified. Toolchain and dependencies are offline.
"""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import argparse
from support.spawn_fixture import enabled_kinds, singleton_items

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--foundation', action='store_true', help='Measure the staged R3 foundation instead')
parser.add_argument('--cpu', type=int, help='Pin the simulation process to this CPU')
parser.add_argument('--minutes', type=int, default=8)
parser.add_argument('--case', help='One seed,IQ,deadly_walls case, e.g. 73,100,true')
args = parser.parse_args()

crate = Path(__file__).resolve().parent.parent
env = dict(os.environ, RUSTC='/usr/bin/rustc', RUSTDOC='/usr/bin/rustdoc', CARGO_NET_OFFLINE='true')
scratch = Path.home() / '.cache/agent-scratch/plasma-wayland-screensaver'
scratch.mkdir(parents=True, exist_ok=True)
with tempfile.TemporaryDirectory(prefix='snakes-magnet-matrix-', dir=scratch) as root:
    isolated = Path(root) / 'rust/snakes-core'
    shutil.copytree(crate, isolated, ignore=shutil.ignore_patterns('target', '__pycache__'))
    shutil.copytree(crate.parent.parent / 'src/shaders', Path(root) / 'src/shaders')
    if args.foundation:
        for relative in ['src/world/effects/mod.rs', 'src/world/effects/magnet.rs', 'src/ai/mod.rs']:
            original = subprocess.check_output(['git', 'show', ':rust/snakes-core/' + relative], cwd=crate)
            (isolated / relative).write_bytes(original)
    hooks = isolated / 'src/world/effects/mod.rs'
    # Foundation retains its production spawn set; validate the declaration
    # without assuming its contents. The normal experiment enables MAGNET only.
    source = enabled_kinds(hooks.read_text(), 'Magnet')
    if not args.foundation:
        hooks.write_text(source)
    items = isolated / 'src/world/items.rs'
    source = singleton_items(items.read_text())
    if not args.foundation:
        items.write_text(source)
    env['CARGO_TARGET_DIR'] = str(Path(root) / 'target')
    subprocess.run(['/usr/bin/cargo', 'build', '--frozen', '--offline', '--release',
                    '--manifest-path', str(isolated / 'Cargo.toml'), '--example', 'magnet_scorecard'],
                   env=env, check=True, timeout=600)
    command = [str(Path(env['CARGO_TARGET_DIR']) / 'release/examples/magnet_scorecard'), str(args.minutes)]
    if args.case:
        seed, iq, walls = args.case.split(',')
        if walls not in ('true', 'false'):
            parser.error('--case deadly_walls must be true or false')
        command += [seed, iq, 'deadly' if walls == 'true' else 'wrap']
    command += ['--ai-only']
    if args.cpu is not None:
        command = ['taskset', '-c', str(args.cpu)] + command
    print('foundation' if args.foundation else 'MAGNET-only', 'CPU', args.cpu, flush=True)
    subprocess.run(command, env=env, check=True, timeout=900)

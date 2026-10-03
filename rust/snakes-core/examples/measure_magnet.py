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

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--foundation', action='store_true', help='Measure the staged R3 foundation instead')
parser.add_argument('--cpu', type=int, help='Pin the simulation process to this CPU')
args = parser.parse_args()

crate = Path(__file__).resolve().parent.parent
env = dict(os.environ, RUSTC='/usr/bin/rustc', RUSTDOC='/usr/bin/rustdoc', CARGO_NET_OFFLINE='true')
with tempfile.TemporaryDirectory(prefix='snakes-magnet-matrix-') as root:
    isolated = Path(root) / 'rust/snakes-core'
    shutil.copytree(crate, isolated, ignore=shutil.ignore_patterns('target'))
    shutil.copytree(crate.parent.parent / 'src/shaders', Path(root) / 'src/shaders')
    if args.foundation:
        for relative in ['src/world/effects/magnet.rs', 'src/ai/mod.rs']:
            original = subprocess.check_output(['git', 'show', ':rust/snakes-core/' + relative], cwd=crate)
            (isolated / relative).write_bytes(original)
    hooks = isolated / 'src/world/effects/mod.rs'
    source = hooks.read_text()
    old = '&[EffectKind::Surge, EffectKind::Magnet, EffectKind::Phase]'
    assert old in source, 'Update the fixture for the current enabled spawn set'
    if not args.foundation:
        hooks.write_text(source.replace(old, '&[EffectKind::Magnet]'))
    items = isolated / 'src/world/items.rs'
    source = items.read_text()
    assert 'if total==0 {return;}' in source
    # Reset only the exclusion history for this singleton measurement fixture.
    source = source.replace('let total: u32=', 'self.last_item_kind = EffectKind::None;\n        let total: u32=')
    if not args.foundation:
        items.write_text(source)
    env['CARGO_TARGET_DIR'] = str(Path(root) / 'target')
    subprocess.run(['/usr/bin/cargo', 'build', '--frozen', '--offline', '--release',
                    '--manifest-path', str(isolated / 'Cargo.toml'), '--example', 'magnet_scorecard'],
                   env=env, check=True)
    command = [str(Path(env['CARGO_TARGET_DIR']) / 'release/examples/magnet_scorecard'), '8', '--ai-only']
    if args.cpu is not None:
        command = ['taskset', '-c', str(args.cpu)] + command
    print('foundation' if args.foundation else 'MAGNET-only', 'CPU', args.cpu, flush=True)
    subprocess.run(command, env=env, check=True)

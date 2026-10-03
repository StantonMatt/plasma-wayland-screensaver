#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Run through heavy. Build immutable offline snapshots and compare paired matrices.

Singleton fixtures permit repeated item kinds; production keeps its exclusion.
Foundation uses the staged sources of an explicit untouched foundation worktree.
"""
import argparse
import os
from pathlib import Path
import re
import shutil
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--modes', nargs='+', default=['foundation', 'combined', 'surge', 'magnet', 'phase'])
parser.add_argument('--foundation-tree', type=Path)
parser.add_argument('--minutes', type=int, default=8)
parser.add_argument('--cpu', type=int)
parser.add_argument('--repeats', type=int, default=1)
args = parser.parse_args()
source = Path(__file__).resolve().parents[1]
args.output.mkdir(parents=True, exist_ok=True)
env = dict(os.environ, RUSTC='/usr/bin/rustc', RUSTDOC='/usr/bin/rustdoc', CARGO_NET_OFFLINE='true')
binaries = {}
for mode in args.modes:
    crate = args.output / mode / 'rust/snakes-core'
    if crate.exists():
        raise SystemExit(f'Refusing to replace existing snapshot: {crate}')
    shutil.copytree(source, crate, ignore=shutil.ignore_patterns('target', '__pycache__'))
    shutil.copytree(source.parents[1] / 'src/shaders', args.output / mode / 'src/shaders')
    if mode == 'foundation':
        if not args.foundation_tree:
            raise SystemExit('--foundation-tree required for foundation')
        for relative in ['src/ai/mod.rs', 'src/ai/attack.rs', 'src/world/effects/mod.rs', 'src/world/effects/surge.rs',
                         'src/world/effects/magnet.rs', 'src/world/effects/phase.rs']:
            data = subprocess.check_output(['git', 'show', ':rust/snakes-core/' + relative], cwd=args.foundation_tree)
            (crate / relative).write_bytes(data)
    elif mode != 'combined':
        if mode not in ['surge', 'magnet', 'phase']:
            raise SystemExit(f'Unknown mode {mode}')
        hooks = crate / 'src/world/effects/mod.rs'
        hooks.write_text(re.sub(r'pub const ENABLED_KINDS: &\[EffectKind\] = &\[[^;]+;',
            f'pub const ENABLED_KINDS: &[EffectKind] = &[EffectKind::{mode.title()}];', hooks.read_text()))
        items = crate / 'src/world/items.rs'
        text = items.read_text()
        text = text.replace('filter(|&&k|k!=self.last_item_kind)', 'filter(|&&k|effects::ENABLED_KINDS.len()==1 || k!=self.last_item_kind)')
        text = text.replace('if k==self.last_item_kind {continue;}', 'if effects::ENABLED_KINDS.len()>1 && k==self.last_item_kind {continue;}')
        items.write_text(text)
    subprocess.run(['/usr/bin/cargo', 'build', '--release', '--frozen', '--offline', '--manifest-path',
                    str(crate / 'Cargo.toml'), '--example', 'r3_scorecard'], env=env, check=True, timeout=600)
    binaries[mode] = crate / 'target/release/examples/r3_scorecard'
for repeat in range(args.repeats):
    for mode, binary in binaries.items():
        command = [str(binary), str(args.minutes)]
        if args.cpu is not None:
            command = ['taskset', '-c', str(args.cpu)] + command
        log = args.output / f'{mode}-{repeat}.txt'
        print(f'Measuring {mode} repeat={repeat} log={log}', flush=True)
        with log.open('w') as stream:
            subprocess.run(command, env=env, stdout=stream, check=True, timeout=900)
        print('\n'.join(log.read_text().splitlines()[-2:]), flush=True)

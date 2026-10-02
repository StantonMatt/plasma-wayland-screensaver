#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Replay recorded AI-free JS fixtures through the feature-gated Rust binary."""
import argparse
import json
import math
from pathlib import Path
import subprocess
import sys

sys.dont_write_bytecode = True

from fixtures import HERE, load_fixture

TOLERANCE = 1e-6  # Fixtures round to 1e-6; allow last-bit libm differences.


def numeric_input(f):
    """Protocol v1: config, complete initial mechanics state, then tick commands."""
    values = [1]

    def add(*items):
        values.extend(int(x) if isinstance(x, bool) else x for x in items)

    c, s = f['config'], f['initialState']
    # Every palette in the recorded VisualUtils.colors implementation has six entries.
    add(*f['world'], f['seed'], c['animationDensity'], c['trailAmount'],
        c['animationScale'], c['animationSpeed'], c['snakeIntelligence'], 6,
        c['snakeSelfCollisions'], c['snakeDeadlyWalls'], f['ticks'], f['dt'],
        f['operation'] == 'collision-only')
    add(s['randomState'], f['frames'][0]['rngDraws'], s['simulationTime'],
        s['nextFoodId'], s['nextFeastId'], s['growthSlots'], len(s['snakes']))
    for snake in s['snakes']:
        for key in ('alive', 'respawn', 'angle', 'desiredAngle', 'baseRadius',
                    'radius', 'birthLength', 'colorIndex', 'speedBias', 'turnBias',
                    'wanderPhase', 'aggression', 'brainCooldown', 'growth',
                    'growthStretch', 'growthBlocked', 'rush', 'score'):
            add(snake[key])
        add(len(snake['segments']))
        for p in snake['segments']:
            add(*(p[k] for k in ('x', 'y', 'previousX', 'previousY')))
        add(snake['trailStart'], len(snake['trailPoints']))
        for p in snake['trailPoints']:
            add(p['x'], p['y'], p['distance'])
    add(len(s['food']))
    for p in s['food']:
        add(*(p[k] for k in ('id', 'x', 'y', 'value', 'colorIndex', 'size', 'vx',
                            'vy', 'life', 'phase', 'feastId', 'trailIndex',
                            'feastLength', 'attraction', 'attractionX',
                            'attractionY', 'vacuumOwner', 'vacuumOriginalLife')))
    samples = {frame['tick'] for frame in f['frames']}
    actions = {}
    for frame in f['frames']:
        for e in frame['events']:
            if e['type'] in ('resize', 'setHead', 'setSegment'):
                actions.setdefault(e['tick'], []).append(e)
    for tick in range(1, f['ticks']+1):
        add(tick in samples, len(actions.get(tick, [])))
        for e in actions.get(tick, []):
            if e['type'] == 'resize':
                add(1, *e['world'])
            else:
                add(2 if e['type'] == 'setHead' else 3, e['snake'],
                    e.get('segment', 0), *e['position'])
        for i in range(len(s['snakes'])):
            if f['script'] == 'weave-quarter-turn-v1':
                angle = i*2.399963229728653 + 0.7*math.sin(tick/30*0.8+i) + (tick//90 % 4)*math.pi/2
            elif f['id'] == 'vacuum-lock':
                angle = 0 if tick < 2 else math.pi
            elif f['id'] in ('head-on-equal', 'head-on-length-difference', 'wrap-seam-collision'):
                angle = 0 if i == 0 else math.pi
            elif f['id'] == 'body-hit':
                angle = 0 if i == 0 else -math.pi/2
            else:
                angle = 0
            # Match JS normalizeAngle's repeated subtraction, including +pi.
            while angle > math.pi:
                angle -= math.tau
            while angle < -math.pi:
                angle += math.tau
            add(angle)
    return '\n'.join(map(str, values))+'\n'


def compare_fixture(f, frames):
    errors = []
    max_error = 0.0
    max_early_error = 0.0
    max_path = ''
    first_divergence = None
    discrete = {'tick', 'rngState', 'rngDraws', 'nextFoodId', 'nextFeastId',
                'index', 'alive', 'length', 'snake', 'food', 'killer',
                'killerCandidates', 'emittedFood', 'type', 'reason', 'segment'}

    def fail(message, tick):
        nonlocal first_divergence
        if first_divergence is None:
            first_divergence = tick
        errors.append(message)

    def compare(a, b, path, key='', exact=False, tick=0):
        nonlocal max_error, max_early_error, max_path, first_divergence
        exact = exact or key in discrete
        if isinstance(a, dict):
            if not isinstance(b, dict) or a.keys() != b.keys():
                fail(f'{path}: keys differ', tick)
                return
            for k in a:
                compare(a[k], b[k], f'{path}.{k}', k, exact, tick)
        elif isinstance(a, list):
            if not isinstance(b, list) or len(a) != len(b):
                fail(f'{path}: lengths differ ({len(a)} vs {len(b) if isinstance(b, list) else b})', tick)
                return
            for i, (x, y) in enumerate(zip(a, b)):
                # Food tuple IDs and capture ownership are exact.
                item_exact = exact or (key == 'particle' and i in (0, 4))
                compare(x, y, f'{path}[{i}]', 'particle' if key == 'particles' else '', item_exact, tick)
        elif isinstance(a, (float, int)) and not isinstance(a, bool) and not exact:
            if not isinstance(b, (float, int)) or not math.isfinite(b):
                fail(f'{path}: nonnumeric/nonfinite {b}', tick)
                return
            error = abs(a-b)
            if tick <= 300:
                max_early_error = max(max_early_error, error)
            if error > max_error:
                max_error, max_path = error, path
            if error > TOLERANCE:
                fail(f'{path}: {a} vs {b} (error {error:.9g})', tick)
        elif a != b:
            fail(f'{path}: {a} vs {b}', tick)

    if len(frames) != len(f['frames']):
        return [f'frame count {len(frames)} != {len(f["frames"])}'], 0, 0, '', None
    for expected, actual in zip(f['frames'], frames):
        # 'food' denotes both an exact event ID and a continuous tuple array.
        expected = dict(expected)
        actual = dict(actual)
        food_expected, food_actual = expected.pop('food'), actual.pop('food')
        tick = expected['tick']
        compare(expected, actual, f'tick {tick}', tick=tick)
        compare(food_expected, food_actual, f'tick {tick}.food', key='particles', tick=tick)
    return errors, max_error, max_early_error, max_path, first_divergence


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True, type=Path)
    parser.add_argument('fixtures', nargs='*', type=Path)
    args = parser.parse_args()
    paths = args.fixtures or sorted((HERE/'fixtures').glob('*.json*'))
    failed = 0
    for path in paths:
        f = load_fixture(path)
        result = subprocess.run([str(args.binary.resolve())], input=numeric_input(f),
                                text=True, capture_output=True, timeout=60)
        if result.returncode:
            print(f'FAIL {f["id"]}: runner exited {result.returncode}: {result.stderr.strip()}')
            failed += 1
            continue
        frames = [json.loads(line) for line in result.stdout.splitlines()]
        errors, maximum, early, location, divergence = compare_fixture(f, frames)
        if errors:
            failed += 1
        events = sum(len(frame['events']) for frame in frames)
        print(f'{"FAIL" if errors else "PASS"} {f["id"]}: {f["ticks"]} ticks, {len(frames)} samples, {events} events; max error {maximum:.9g} at {location}; first 300 max {early:.9g}; first divergence {divergence}')
        for error in errors[:5]:
            print('  '+error)
        if len(errors) > 5:
            print(f'  ... {len(errors)} differences')
    print(f'{len(paths)-failed}/{len(paths)} fixtures passed (continuous tolerance {TOLERANCE:g}; discrete outcomes exact)')
    return bool(failed)


if __name__ == '__main__':
    sys.exit(main())

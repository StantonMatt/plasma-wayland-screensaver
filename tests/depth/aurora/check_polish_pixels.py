#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Guard C/75 diffuse-island fading and live composite dither in native captures."""
import argparse
import json
import subprocess
from pathlib import Path
from PIL import Image, ImageStat


def island(root):
    path = root / 'scenes/C/75000000000-world.png'
    with Image.open(path) as image:
        crop = image.convert('RGB').crop((540, 140, 800, 460))
    rgb = ImageStat.Stat(crop).mean
    return sum(value * weight for value, weight in zip(rgb, (0.2126, 0.7152, 0.0722)))


def live_composite_share(video):
    # G's upper sky has no moving light in this box. A live blue-noise offset
    # changes it; a composite material retaining its first uniform block does not.
    raw = subprocess.check_output([
        'ffmpeg', '-v', 'error', '-threads', '2', '-i', str(video),
        '-vf', 'crop=200:100:1000:40', '-frames:v', '2', '-threads', '2',
        '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-',
    ])
    size = 200 * 100 * 3
    if len(raw) != 2 * size:
        raise ValueError('Expected two native G frames')
    changed = sum(raw[i:i+3] != raw[size+i:size+i+3] for i in range(0, size, 3))
    return changed / (200 * 100)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('before', type=Path)
    parser.add_argument('after', type=Path)
    parser.add_argument('--live-video', type=Path, help='native G movie at 2560x1440, 30 fps')
    args = parser.parse_args()
    requests = [json.loads((root / 'scenes/C/request.json').read_text())
                for root in (args.before, args.after)]
    for key in ('seed', 'settings', 'layout', 'monitorMode', 'events', 'outputs'):
        if requests[0][key] != requests[1][key]:
            raise ValueError(f'Unmatched C captures: {key}')
    before, after = island(args.before), island(args.after)
    result = dict(beforeMean=before, afterMean=after, ratio=after / before,
                  maximumRatio=0.90, passed=after <= 0.90 * before)
    if args.live_video:
        share = live_composite_share(args.live_video)
        result.update(liveCompositeChangedShare=share, minimumLiveShare=0.05)
        result['passed'] = result['passed'] and share >= 0.05
    print(json.dumps(result, indent=2))
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())

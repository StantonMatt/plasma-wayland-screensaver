#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Offline gate on the encoded review media, at native resolution and frame rate."""
import argparse
import json
import math
from fractions import Fraction
import subprocess
from pathlib import Path


def field_args(scene, seam, width, height):
    """Map each physical output and its full 10-degree window into the encoded crop."""
    outputs = scene.get('outputs', []) if scene else []
    if not outputs:
        return []
    x0 = min(o['x'] for o in outputs)
    y0 = min(o['y'] for o in outputs)
    full_w = max(o['x'] + o['width'] for o in outputs) - x0
    full_h = max(o['y'] + o['height'] for o in outputs) - y0
    crop = scene['video'].get('crop')
    if seam and crop is None:
        # The first Constellation capture predates the crop field. Its native
        # 768x960 detail is centred on the first seam of the 7920x1440 layout.
        if (scene.get('visual'), scene.get('layout'), full_w, full_h, width, height) != (
                'constellation', 'l3', 7920, 1440, 768, 960):
            raise ValueError('Missing seam crop in capture manifest')
        crop = (2176, 240, 768, 960)
    cx, cy, cw, ch = crop if seam else (0, 0, full_w, full_h)
    sx, sy = width / cw, height / ch
    fields = []
    for o in outputs:
        x, y = o['x'] - x0 - cx, o['y'] - y0 - cy
        left, top = max(0, math.floor(x*sx)), max(0, math.floor(y*sy))
        right, bottom = min(width, math.ceil((x+o['width'])*sx)), min(height, math.ceil((y+o['height'])*sy))
        if right > left and bottom > top:
            fields.extend([left, top, right-left, bottom-top,
                           max(1, math.floor(o['width']*sx*341/1024 + 0.5)),
                           max(1, math.floor(o['height']*sy*256/768 + 0.5))])
    if not fields:
        raise ValueError('Capture contains no declared output area')
    return list(map(str, fields))


def check(video, binary, scene=None):
    metadata = json.loads(subprocess.check_output(['ffprobe', '-v', 'error', '-select_streams', 'v:0',
        '-show_entries', 'stream=width,height,avg_frame_rate,nb_frames', '-of', 'json', str(video)]))['streams'][0]
    fps = Fraction(metadata['avg_frame_rate'])
    if fps.denominator != 1:
        raise ValueError('Capture must have a fixed integer frame rate')
    with subprocess.Popen(['ffmpeg', '-v', 'error', '-threads', '4', '-i', str(video), '-map', '0:v:0',
            '-threads','4','-f', 'rawvideo', '-pix_fmt', 'rgba', '-'], stdout=subprocess.PIPE) as decoder:
        result = subprocess.run([str(binary), str(metadata['width']), str(metadata['height']), str(fps),
                                 *field_args(scene, video.stem == 'seam', metadata['width'], metadata['height'])],
                                stdin=decoder.stdout, capture_output=True, text=True)
        decoder.stdout.close()
        if decoder.wait() or result.returncode not in (0,1):
            raise RuntimeError('Incomplete decode or checker failure')
    measurements = json.loads(result.stdout)
    if measurements['frames'] != int(metadata['nb_frames']):
        raise RuntimeError('Decoded frame count differs from stream')
    return dict(path=str(video), passed=result.returncode == 0, **measurements)


def check_sequence(manifest, stream, binary):
    from PIL import Image
    directory = manifest.parent
    scene = json.loads(manifest.read_text())
    paths = sorted(directory.glob(stream['sequence']))
    fps = scene['video']['fps']
    if len(paths) != scene['videoFrames'] or not paths:
        raise RuntimeError('Incomplete PNG sequence')
    with Image.open(paths[0]) as first:
        width, height = first.size
    with subprocess.Popen([str(binary), str(width), str(height), str(fps),
                           *field_args(scene, stream['sequence'].startswith('seam-'), width, height)],
                          stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=False) as checker:
        for i, path in enumerate(paths):
            if int(path.stem.rsplit('-', 1)[1]) != i:
                raise RuntimeError('Missing PNG frame')
            with Image.open(path) as frame:
                if frame.size != (width, height):
                    raise RuntimeError('PNG size changed during recording')
                checker.stdin.write(frame.convert('RGBA').tobytes())
        checker.stdin.close()
        measurements = json.loads(checker.stdout.read())
        code = checker.wait()
    if code not in (0, 1) or measurements['frames'] != len(paths):
        raise RuntimeError('Incomplete luminance analysis')
    return dict(path=str(directory / stream['sequence']), passed=code == 0, **measurements)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--build', type=Path, required=True)
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('videos', type=Path, nargs='+')
    args = parser.parse_args()
    binary = args.build / 'bin/depth-luminance'
    reports = []
    for path in args.videos:
        if path.suffix == '.json':
            scene = json.loads(path.read_text())
            for stream in scene['recordings']:
                reports.append(check(path.parent / stream, binary, scene) if isinstance(stream, str)
                               else check_sequence(path, stream, binary))
        else:
            reports.append(check(path, binary))
    if not reports:
        parser.error('No capture streams to check')
    data = json.dumps(reports, indent=2)
    args.report.write_text(data+'\n')
    print(data)
    raise SystemExit(not all(r['passed'] for r in reports))


#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Scan every decoded frame for an isolated discontinuity in the flight."""
import argparse
import json
from pathlib import Path
from statistics import median
import subprocess


def differences(inputs):
    # Compare frame n to n+1, retaining the native raster. msad exports a
    # normalized absolute luminance difference for every matched pair.
    graph = ('format=gray,split[old][next];[next]trim=start_frame=1,setpts=PTS-STARTPTS[new];'
             '[old][new]msad=shortest=1,metadata=print:key=lavfi.msad.msad.Y:file=-')
    result = subprocess.run(['ffmpeg', '-nostdin', '-v', 'error', '-threads', '3', *inputs,
                             '-filter_complex_threads', '2', '-filter_complex', graph,
                             '-an', '-f', 'null', '-'], capture_output=True, text=True, check=True, timeout=1200)
    key = 'lavfi.msad.msad.Y='
    return [255 * float(line[len(key):]) for line in result.stdout.splitlines() if line.startswith(key)]


def transitions(scene):
    # Natural scene G is pinned to the reference seed and epoch. Other shots
    # use the capture catalog's explicit start-of-spool event.
    if scene['scene'] == 'G-motion':
        return [(639.45, 640.30)]
    starts = [int(e['timelineNs']) * 1e-9 for e in scene['events'] if e['id'] == 1]
    return [interval for t in starts for interval in ((t+3, t+3.85), (t+20, t+20.95))]


def scan(movie, inputs, scene):
    values = differences(inputs)
    video = scene['video']
    fps = video['fps']
    assert len(values) == scene['videoFrames'] - 1, (movie, len(values), scene['videoFrames'])
    excluded = transitions(scene)
    worst = dict(ratio=0.)
    worst_ratio = -1.
    spikes = []
    for i, change in enumerate(values):
        frame = i + 1
        t = video['start'] + frame / fps
        if any(lo <= t <= hi for lo, hi in excluded):
            continue
        peers = values[max(0, i-5):i] + values[i+1:i+6]
        floor = median(peers)
        ratio = change / floor if floor else (float('inf') if change else 0.)
        if ratio > worst_ratio or ratio > 3.:
            hit = dict(frame=frame, voyageSeconds=round(t, 4), meanAbsoluteStep=round(change, 5),
                       neighbouringMedian=round(floor, 5), ratio=round(ratio, 4) if floor or not change else None)
            if ratio > worst_ratio:
                worst_ratio, worst = ratio, hit
            if ratio > 3.:
                spikes.append(hit)
    return dict(movie=str(movie), frames=len(values)+1, fps=fps, worst=worst, spikes=spikes)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('round', type=Path)
    parser.add_argument('--report', required=True, type=Path)
    args = parser.parse_args()
    reports = []
    for relative in json.loads((args.round/'manifest.json').read_text())['scenes']:
        path = args.round/relative
        scene = json.loads(path.read_text())
        if 'video' not in scene:
            continue
        video = scene['video']
        stems = [video.get('name', 'world')] + (['seam'] if video.get('seam') else [])
        streams = scene['recordings']
        if len(streams) != len(stems):
            raise ValueError(f'Missing review film in {path}')
        for stem, stream in zip(stems, streams, strict=True):
            if isinstance(stream, str):
                movie = path.parent / stream
                inputs = ['-i', str(movie)]
            else:
                pattern = stem + '-' + '[0-9]' * 5 + '.png'
                if stream['sequence'] != pattern or len(list(path.parent.glob(pattern))) != video['frames']:
                    raise ValueError(f'Incomplete PNG sequence in {path}')
                movie = path.parent / (stem + '-%05d.png')
                inputs = ['-framerate', str(video['fps']), '-i', str(movie)]
            result = scan(movie, inputs, scene)
            reports.append(result)
            print(f"{movie.name}  {result['frames']} frames, {len(result['spikes'])} spikes, worst {result['worst']}", flush=True)
    assert reports, 'No review films were scanned'
    args.report.write_text(json.dumps(reports, indent=2)+'\n')
    return int(any(row['spikes'] for row in reports))


if __name__ == '__main__':
    raise SystemExit(main())

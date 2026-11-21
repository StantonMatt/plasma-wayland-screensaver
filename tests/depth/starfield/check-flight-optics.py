#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Measure the warp light and the 30 fps arrival directly in the lossless review media."""
import argparse
import io
import json
import math
from pathlib import Path
from statistics import median
import subprocess

from PIL import Image


def movie_crop(path, seconds, box):
    x, y, w, h = box
    args = ['ffmpeg', '-v', 'error', '-ss', str(seconds), '-i', str(path),
            '-vf', f'crop={w}:{h}:{x}:{y}', '-frames:v', '1', '-threads', '1',
            '-f', 'image2pipe', '-vcodec', 'png', '-']
    return Image.open(io.BytesIO(subprocess.check_output(args, timeout=90))).convert('RGB')


def percentile(values, fraction):
    ordered = sorted(values)
    return ordered[math.floor(fraction * (len(ordered) - 1))]


def fit_light(radii, levels):
    # Separate the measured narrow core from the halo. Both amplitudes are
    # fitted from the PNG; the 3.8 px core width is the retained optical core.
    core = [math.exp(-0.5 * (r / 3.8)**2) for r in radii]
    aa = sum(v*v for v in core)
    ay = sum(a*y for a,y in zip(core,levels))
    best = None
    for width in range(5, 85):
        for shift in (0., 0.5, 1., 1.5):
            # Exclude fits that simply swap the core and skirt in a dim frame.
            if width*(math.sqrt(shift**2+2*math.log(2))-shift)<6:
                continue
            halo = [math.exp(-0.5*(r/width)**2-shift*r/width) for r in radii]
            ab = sum(a*b for a,b in zip(core,halo))
            bb = sum(b*b for b in halo)
            by = sum(b*y for b,y in zip(halo,levels))
            det = aa*bb-ab*ab
            a = (ay*bb-by*ab)/det
            b = (aa*by-ab*ay)/det
            if min(a,b) < 0:
                continue
            error = sum((y-a*v-b*q)**2 for y,v,q in zip(levels,core,halo))
            if best is None or error < best[0]:
                best = (error, width, shift, a, b)
    error,width,shift,a,b = best
    half_radius = width*(math.sqrt(shift**2+2*math.log(2))-shift)
    return dict(haloPeak=round(b,2), haloHalfMaximumPx=round(half_radius,2),
                fitSigmaPx=width, fitShift=shift, fitRms=round(math.sqrt(error/len(radii)),2))


def bloom(im, expected, height, scale, threshold=120):
    im = im.convert('RGB')
    gray = im.convert('L')
    # Refine the approximate camera coordinate using only the hot core.
    cx, cy = expected
    mass = []
    for y in range(cy - 5, cy + 6):
        for x in range(cx - 5, cx + 6):
            mass.append((x + 0.5, y + 0.5, max(gray.getpixel((x, y)) - threshold, 0)))
    total = sum(p[2] for p in mass)
    assert total, 'Missing central core'
    cx, cy = [sum(p[i] * p[2] for p in mass) / total for i in (0, 1)]
    rings = [[] for _ in range(25)]
    distances = [[] for _ in rings]
    channels = [[[] for _ in rings] for _ in range(3)]
    for y in range(math.floor(cy - 100), math.ceil(cy + 100)):
        for x in range(math.floor(cx - 100), math.ceil(cx + 100)):
            distance = math.hypot(x + 0.5 - cx, y + 0.5 - cy)
            k = int(distance / 4)
            if k < len(rings):
                rings[k].append(gray.getpixel((x, y)))
                distances[k].append(distance)
                for stream, value in zip(channels,im.getpixel((x,y))):
                    stream[k].append(value)
    profile = [percentile(ring, 0.20) for ring in rings]
    background = median(profile[21:])
    levels = [round(v - background, 2) for v in profile]
    drops = [round(a - b, 2) for a, b in zip(levels, levels[1:])]
    radii = [percentile(rr, 0.80) for rr in distances]
    fit = fit_light(radii, levels)
    peaks = []
    for stream in channels:
        profile_rgb = [percentile(segment,0.20) for segment in stream]
        baseline = median(profile_rgb[21:])
        peaks.append(fit_light(radii,[v-baseline for v in profile_rgb])['haloPeak'])
    half = next(i for i, v in enumerate(levels) if v <= 0.5*levels[0])
    # Bound the crossing conservatively by the outside of its four-pixel ring.
    half_upper = 4 * (half + 1)
    at_four = levels[int(0.04 * height / 4)]
    return dict(**fit, ringWidth=4, p20Excess=levels, drops=drops, background=background,
                corePeak=max(v+threshold for _, _, v in mass), coreRadiusAt160=round(math.sqrt(sum(v+threshold>=160 for _,_,v in mass)/math.pi),2),
                haloPeakRGB=peaks, halfMaximumUpperPx=half_upper,
                halfLimitPx=round(0.013 * height * scale, 2), excessAtFourPercent=at_four,
                largestRise=max(-d for d in drops[:18]),
                largestIncreaseInDrop=max(b-a for a,b in zip(drops[1:17],drops[2:18])))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('round', type=Path)
    parser.add_argument('--g-film', type=Path)
    parser.add_argument('--g-start', type=float, default=630)
    parser.add_argument('--report', required=True, type=Path)
    args = parser.parse_args()
    scenes = args.round/'scenes'
    with Image.open(scenes/'S2/40000000000-world.png') as im:
        s2 = bloom(im, (1331, 716), 1440, 1)
    with Image.open(scenes/'trails-0/40000000000-world.png') as im:
        short = bloom(im, (1331,716),1440,1,threshold=42)
    g = args.g_film or scenes/'G-motion/G-motion.mp4'
    offset = (1088, 626)
    patch = movie_crop(g, 641-args.g_start, (*offset, 272, 272))
    result = {'bloom': {'S2': s2, 'Trails0': short, 'G641': bloom(patch,(1219-offset[0],759-offset[1]),1440,2)}}
    args.report.write_text(json.dumps(result, indent=2)+'\n')
    for name, row in result['bloom'].items():
        print(name, row)
        # A one-code rounding interval covers two adjacent quantized readings.
        if name=='Trails0':
            continue
        assert row['largestRise'] <= 1 and row['largestIncreaseInDrop'] <= 1, name
        assert 3 <= row['coreRadiusAt160'] <= 5.2 and row['halfMaximumUpperPx'] <= row['halfLimitPx'], name
        assert row['haloPeak'] <= 30 and row['haloHalfMaximumPx'] <= row['halfLimitPx'], name
        assert row['fitRms'] <= 1.5 and row['excessAtFourPercent'] <= 1, name
        red,green,blue=row['haloPeakRGB']
        assert max(red,green,blue)<=30, (name,row['haloPeakRGB'])
        if name=='S2':
            assert red<0.55*blue and green>red and blue>=green, row
        if name=='G641':
            assert blue<0.6*red and red>green>blue, row
    # Relative exposure in the central four-pixel ring has ample signal even
    # with the short setting. Both core and skirt use this multiplier.
    ratio=short['p20Excess'][0]/s2['p20Excess'][0]
    assert 0.26 <= ratio <= 0.34, (short,s2)


if __name__ == '__main__':
    main()

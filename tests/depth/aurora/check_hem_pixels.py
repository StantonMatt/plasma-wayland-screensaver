#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Trace the lower optical edge in native Aurora captures. Only Pillow is required."""
import argparse
import cmath
import json
import math
from pathlib import Path

from PIL import Image

# Crossings have multiple legitimate hems. Follow the indicated visible branch
# from left to right, using a wide vertical allowance for changes in the profile.
CASES = [
    ('A', 30, [(1320, 695), (1370, 749), (1420, 800), (1450, 820)]),
    ('F', 0, [(2000, 798), (2040, 765), (2080, 709), (2140, 645), (2210, 585)]),
    ('F', 630, [(990, 385), (1050, 450), (1110, 542), (1180, 648), (1250, 735)]),
    ('C', 55, [(705, 650), (770, 696), (830, 747), (850, 760)]),
    ('active', 80, [(1360, 693), (1410, 723), (1460, 765), (1500, 804)]),
]


def trace(image, path):
    pixels = image.getchannel('G').load()
    heights = []
    last = path[0][1]
    for (x0, y0), (x1, y1) in zip(path, path[1:]):
        for x in range(x0, x1):
            centre = y0 + (x-x0) * (y1-y0) / (x1-x0)
            radius = 75
            def drop(y):
                return pixels[x, y-3] - pixels[x, y+3]
            lo, hi = round(centre)-radius, round(centre)+radius
            y = max(range(lo, hi+1), key=lambda j: drop(j)-0.06*abs(j-centre)-0.32*abs(j-last))
            # Centroid of the lower edge spans six camera pixels. The unsharp
            # digital derivative alone snaps to integer coordinates on soft hems.
            weights = [max(0, drop(k))**2 for k in range(y-4, y+5)]
            total = sum(weights)
            if not total:
                raise ValueError(f'No optical edge near {(x,y)}')
            last = sum(k*v for k,v in zip(range(y-4,y+5), weights)) / total
            heights.append(last)
    return heights


def residuals(heights):
    # Quadratic Savitzky-Golay fit, evaluated at the centre and differentiated.
    side = 17
    offsets = range(-side, side+1)
    n = 2*side+1
    s2 = sum(k*k for k in offsets)
    s4 = sum(k**4 for k in offsets)
    intercept = [(s4-s2*k*k)/(n*s4-s2*s2) for k in offsets]
    out = []
    for i in range(side, len(heights)-side):
        ys = heights[i-side:i+side+1]
        slope = sum(k*y for k,y in zip(offsets,ys))/s2
        expected = sum(k*y for k,y in zip(intercept,ys))
        out.append((heights[i]-expected)/math.hypot(1,slope))
    return out


def measure(image, path):
    residual = residuals(trace(image, path))
    n = len(residual)
    rms = math.sqrt(sum(v*v for v in residual)/n)
    amp, period = max((2*abs(sum(v*cmath.exp(-2j*math.pi*i/p)
                                 for i,v in enumerate(residual)))/n, p)
                      for p in range(8,31))
    return {'normalRmsPixels': round(rms,3), 'periodicAmplitudePixels': round(amp,3),
            'strongestPeriodPixels': period, 'samples': n}



def longest_wall(image, box):
    # Both sides must be inside the light. A sloping hem can briefly cross
    # this horizontal probe, but a vertical cut keeps the same sign for rows.
    strip = image.convert('L').crop(box).transpose(Image.Transpose.TRANSPOSE)
    height, width = strip.size
    pixels = strip.tobytes()
    longest = 0
    for x in range(width-3):
        left = pixels[x*height:(x+1)*height]
        right = pixels[(x+3)*height:(x+4)*height]
        length, prior = 0, 0
        for a, b in zip(left, right):
            sign = (1 if b > a else -1) if min(a, b) > 20 and abs(a-b) > 8 else 0
            length = length+1 if sign and sign == prior else int(bool(sign))
            longest = max(longest, length)
            prior = sign
    return longest


def measure_walls(root):
    regions = [('E-max', 600, (950, 300, 1420, 1000)),
               ('active', 80, (1300, 200, 1780, 900)),
               ('B', 150, (1090, 450, 1590, 970))]
    rows = {}
    for name, t, box in regions:
        with Image.open(root/'scenes'/name/f'{t*10**9}-world.png') as image:
            rows[f'{name}/{t}'] = longest_wall(image, box)
    print(json.dumps(rows, indent=2))
    return int(max(rows.values()) > 60)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('round', type=Path)
    parser.add_argument('--all', action='store_true', help='also test the remaining reviewed steep hems')
    parser.add_argument('--walls', action='store_true', help='measure sustained vertical edges in the three reviewed regions')
    args = parser.parse_args()
    if args.walls:
        return measure_walls(args.round)
    rows = {}
    cases = CASES if args.all else CASES[:2]
    for name, t, path in cases:
        with Image.open(args.round/'scenes'/name/f'{t*10**9}-world.png') as image:
            rows[f'{name}/{t}'] = measure(image.convert('RGB'), path)
    print(json.dumps(rows, indent=2))
    # Allow subpixel sampling noise at native 8-bit precision. The old A/30 and F/0 images
    # have coherent 18 and 17 px components of 2.64 and 1.23 px.
    return int(any(v['normalRmsPixels'] > 1 or v['periodicAmplitudePixels'] > 0.65
                   for v in rows.values()))


if __name__ == '__main__':
    raise SystemExit(main())

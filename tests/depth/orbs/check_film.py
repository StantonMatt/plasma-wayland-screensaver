#!/usr/bin/python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Check one-native-pixel tangents on the four C4 Pastel film captures.

Pass the C4 directory and the geometry written by pastel_film_capture_circles
with PVS_ORBS_FILM_CIRCLES set. Coordinates are in native image pixels.
Exclude intersecting rim edges, specular dots and motes, including the
curved membrane of merging pairs.
Measure crest colour independently at the peak across its radial profile.
"""
import argparse
from collections import defaultdict
from dataclasses import dataclass
from functools import cached_property
import json
import math
from pathlib import Path

from PIL import Image, ImageDraw


@dataclass
class Surface:
    x: float
    y: float
    radius: float
    dx: float
    dy: float
    second: float
    kind: float
    wobble: float

    def __post_init__(self):
        self.modes = [(((int(self.wobble) >> n) & 255) - 128) / 500
                      if self.wobble else 0 for n in (16, 8, 0)]

    def distance(self, angle):
        a, b, c = self.modes
        return self.radius * (1 + a * math.cos(2 * angle) + b * math.sin(2 * angle)
                              + c * math.cos(3 * angle))

    def point(self, angle, fraction, offset=0):
        distance = fraction * self.distance(angle) + offset
        return self.x + distance * math.cos(angle), self.y + distance * math.sin(angle)


    def merged_distance(self, x, y):
        dx, dy = x - self.x, y - self.y
        first = math.hypot(dx, dy) / self.distance(math.atan2(dy, dx))
        second = 1 + (math.hypot(dx-self.dx, dy-self.dy) - self.second) / self.radius
        neck = max(0.28 - abs(first-second), 0)
        return min(first, second) - neck*neck / 1.12

    @cached_property
    def merged_edge(self):
        # The shader blends the two distances in the neck. Mask the contour
        # it actually draws, leaving both lobes' interiors available to test.
        pad = self.radius * 1.4 + 14
        left = math.floor(min(self.x, self.x+self.dx) - pad)
        top = math.floor(min(self.y, self.y+self.dy) - pad)
        right = math.ceil(max(self.x, self.x+self.dx) + pad)
        bottom = math.ceil(max(self.y, self.y+self.dy) + pad)
        edge = Image.new('L', (right-left, bottom-top))
        edge.putdata([255 if abs(self.merged_distance(x+0.5, y+0.5)-0.955)*self.radius <= 10 else 0
                      for y in range(top, bottom) for x in range(left, right)])
        return edge, (left, top)


def exclusions(size, surfaces):
    # Geometry-only mask, independent of the pixel differences under test.
    mask = Image.new('L', size)
    draw = ImageDraw.Draw(mask)

    def disc(x, y, r):
        draw.ellipse((x-r, y-r, x+r, y+r), fill=255)

    for surface in surfaces:
        s = surface
        if s.kind >= 2:
            disc(s.x, s.y, 1.5 * s.radius + 3)
            continue
        disc(s.x - 0.23 * s.radius, s.y - 0.56 * s.radius, 10)
        if s.second:
            edge, origin = s.merged_edge
            mask.paste(255, origin, edge)
            continue
        # Fill quadrilaterals. Thick raster polylines can leave unpainted
        # triangular gaps where their one-pixel segments meet.
        rim = [(s.point(i * math.tau / 240, 0.955, -10),
                s.point(i * math.tau / 240, 0.955, 10)) for i in range(241)]
        for (inside, outside), (next_inside, next_outside) in zip(rim, rim[1:]):
            draw.polygon([inside, outside, next_outside, next_inside], fill=255)
    return mask


def sample(pixels, x, y):
    # Fragment centres lie half a pixel inside the native image.
    x -= 0.5
    y -= 0.5
    i, j = math.floor(x), math.floor(y)
    u, v = x-i, y-j
    a, b, c, d = pixels[i, j], pixels[i+1, j], pixels[i, j+1], pixels[i+1, j+1]
    return tuple((a[k]*(1-u) + b[k]*u)*(1-v) + (c[k]*(1-u) + d[k]*u)*v for k in range(3))


def check_frame(path, surfaces):
    image = Image.open(path).convert('RGB')
    mask = exclusions(image.size, surfaces)
    cover = mask.load()
    pixels = image.load()
    width, height = image.size
    stats = {}

    def observe(band, value, point):
        row = stats.setdefault(band, dict(samples=0, aboveSix=0, maximum=0))
        row['samples'] += 1
        row['aboveSix'] += value > 6.00001
        if value > row['maximum']:
            row['maximum'] = value
            row['at'] = [round(q, 1) for q in point]

    for index, s in enumerate(surfaces):
        if s.kind >= 2 or s.radius < 3:
            continue
        # For this surface alone include its crest, but exclude foreground
        # and background features that intersect the primary rim.
        others = exclusions(image.size, surfaces[:index] + surfaces[index+1:]).load()
        lobes = [(s, 0.955)]
        if s.second:
            secondary = Surface(s.x+s.dx, s.y+s.dy, s.second, 0, 0, 0, 0, 0)
            lobes.append((secondary, 1-0.045*s.radius/s.second))
        for lobe, crest_radius in lobes:
            for fraction, offset in [(0.25, 0), (0.45, 0), (0.65, 0), (0.78, 0),
                                 (0.86, 0), (0.955, 0), (1, 12), (1, 27)]:
                crest = fraction == 0.955
                if crest:
                    fraction = crest_radius
                count = math.ceil(math.tau * (lobe.radius * fraction + offset))
                last = None
                for i in range(count + 1):
                    angle = i * math.tau / count
                    x, y = lobe.point(angle, fraction, offset)
                    if not (4 < x < width-4 and 4 < y < height-4):
                        last = None
                        continue
                    if (others if crest else cover)[int(x), int(y)]:
                        last = None
                        continue
                    near_dot = math.hypot(x-s.x+0.23*s.radius, y-s.y+0.56*s.radius) < 10
                    if crest and (near_dot or (s.second and
                                  abs(s.merged_distance(x, y)-0.955)*s.radius > 0.5)):
                        last = None
                        continue
                    if crest:
                        # Fit the crest from its two-dimensional neighbourhood.
                        # Radial pixel-centre offsets otherwise alias the thin rim.
                        ca, sa = math.cos(angle), math.sin(angle)
                        profile = [sample(pixels, x+j*ca/2, y+j*sa/2) for j in range(-5, 6)]
                        value = max(profile, key=max)
                    else:
                        value = sample(pixels, x, y)
                    if last:
                        # Normalize to a one-pixel tangent on wobbled membranes.
                        length = math.hypot(x-last[1], y-last[2])
                        delta = max(abs(a-b) for a, b in zip(value, last[0])) / length
                        observe('crest' if crest else 'film', delta, (x, y))
                    last = (value, x, y)
    if set(stats) != {'film', 'crest'}:
        raise ValueError(f'No usable rim or film samples in {path}')
    return stats


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('scene', type=Path)
    parser.add_argument('geometry', type=Path)
    parser.add_argument('--report', type=Path)
    args = parser.parse_args()
    surfaces = defaultdict(list)
    for line in args.geometry.read_text().splitlines():
        moment, *record = map(float, line.split())
        surfaces[int(moment)].append(Surface(*record))
    assert sorted(surfaces) == [60, 630, 642, 660]
    results = {s: check_frame(args.scene / f'{s*10**9}-HDMI-A-5.png', rows)
               for s, rows in surfaces.items()}
    body = json.dumps(results, indent=2)
    print(body)
    if args.report:
        args.report.write_text(body + '\n')
    return int(any(row['aboveSix'] or row['samples'] < 1000
                   for frame in results.values() for row in frame.values()))


if __name__ == '__main__':
    raise SystemExit(main())

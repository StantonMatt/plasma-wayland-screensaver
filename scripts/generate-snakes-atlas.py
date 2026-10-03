#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Generate the 128x128 R8 icon SDF. Standard library only, build-time only.

32px padded tiles: bolt, horseshoe, ghost diamonds, fangs, snowflake.
Distances are encoded as 0.5 - distance / 8 pixels (positive inside).
"""
import math
import pathlib
import sys


def segment(x, y, a, b, width):
    dx, dy = b[0] - a[0], b[1] - a[1]
    t = max(0.0, min(1.0, ((x - a[0]) * dx + (y - a[1]) * dy) / (dx * dx + dy * dy)))
    return math.hypot(x - a[0] - t * dx, y - a[1] - t * dy) - width


def polygon(x, y, points):
    distance = min(segment(x, y, a, b, 0) for a, b in zip(points, points[1:] + points[:1]))
    inside = False
    for a, b in zip(points, points[1:] + points[:1]):
        if (a[1] > y) != (b[1] > y) and x < (b[0] - a[0]) * (y - a[1]) / (b[1] - a[1]) + a[0]:
            inside = not inside
    return -distance if inside else distance


def icon(kind, x, y):
    if kind == 0:
        return polygon(x, y, [(2, -11), (-8, 2), (-1, 2), (-3, 11), (8, -3), (1, -3)])
    if kind == 1:
        arc = abs(math.hypot(x, y - 1) - 7) - 2.1 if y >= 1 else 100
        return min(arc, segment(x, y, (-7, 1), (-7, -8), 2.1), segment(x, y, (7, 1), (7, -8), 2.1))
    if kind == 2:
        return min(abs(polygon(x, y, [(c, -10), (c + 4, 0), (c, 10), (c - 4, 0)])) - 1.2 for c in (-4, 4))
    if kind == 3:
        return min(polygon(x, y, [(c - 3, -8), (c + 3, -8), (c * 0.6, 10)]) for c in (-5, 5))
    if kind == 4:
        lines = []
        for arm in range(6):
            angle = arm * math.tau / 6
            direction = math.cos(angle), math.sin(angle)
            lines.append(segment(x, y, (0, 0), (11 * direction[0], 11 * direction[1]), 1.1))
            for side in (-1, 1):
                branch = angle + side * math.pi / 3
                lines.append(segment(x, y, (7 * direction[0], 7 * direction[1]),
                                     (7 * direction[0] - 4 * math.cos(branch), 7 * direction[1] - 4 * math.sin(branch)), 0.9))
        return min(lines)
    return 100


def generate():
    pixels = []
    for y in range(128):
        for x in range(128):
            kind = y // 32 * 4 + x // 32
            d = icon(kind, x % 32 - 15.5, y % 32 - 15.5)
            pixels.append(round(max(0, min(255, 127.5 - d * 255 / 8))))
    return pixels


if __name__ == '__main__':
    pixels = generate()
    target = pathlib.Path(sys.argv[1])
    target.write_text('// SPDX-License-Identifier: GPL-3.0-or-later\n// Generated; do not edit.\n'
                      'static constexpr unsigned char snakesIconAtlas[128*128] = {\n' +
                      '\n'.join(','.join(map(str, pixels[i:i + 128])) + ',' for i in range(0, len(pixels), 128)) + '\n};\n')

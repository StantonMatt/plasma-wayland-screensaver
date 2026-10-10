#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Generate the 128x128 R8 icon SDF. Standard library only, build-time only.

32px padded tiles: six items, one reserved item, then !, ?, anger, Zz, heart, bat, carved pumpkin face.
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


def _quad(p0, c, p2, n=10):
    return [((1-t)**2*p0[0]+2*(1-t)*t*c[0]+t*t*p2[0],
             (1-t)**2*p0[1]+2*(1-t)*t*c[1]+t*t*p2[1])
            for t in (i/n for i in range(n+1))]


_ANGER = _quad((3.6, 11.5), (3.6, 3.6), (11.5, 3.6))


def _bat_points():
    # Right half, traced from the top of the head clockwise to the centre bottom,
    # then mirrored. Units are tile pixels (tile centre 0, content within +-12).
    right = [(0.0, -3.4), (0.9, -4.2), (1.6, -6.8), (2.3, -3.8), (3.2, -3.2)]
    right += _quad((3.2, -3.2), (7.0, -6.8), (12.0, -6.6), 8)[1:]          # leading edge
    right += _quad((12.0, -6.6), (12.0, -3.0), (10.8, -0.4), 4)[1:]        # wing tip
    right += _quad((10.8, -0.4), (9.2, -3.8), (7.6, 0.8), 6)[1:]           # scallop 1
    right += _quad((7.6, 0.8), (6.0, -2.6), (4.4, 1.8), 6)[1:]             # scallop 2
    right += _quad((4.4, 1.8), (3.4, -0.4), (2.2, 2.2), 5)[1:]             # scallop 3
    right += [(1.4, 4.6), (0.0, 5.6)]                                      # body
    left = [(-x, y) for x, y in reversed(right[1:-1])]
    return right + left


_BAT = _bat_points()


def _face(x, y):
    # Positive-outside SDF of the carved openings, as in the base script.
    eye_l = polygon(x, y, [(-8.4, -1.2), (-2.6, -1.2), (-5.5, -7.4)])
    eye_r = polygon(x, y, [(2.6, -1.2), (8.4, -1.2), (5.5, -7.4)])
    nose = polygon(x, y, [(-1.4, 1.4), (1.4, 1.4), (0.0, -1.2)])
    upper = _quad((-9.6, 2.0), (0.0, 5.0), (9.6, 2.0), 10)
    lower = _quad((9.6, 2.0), (0.0, 15.4), (-9.6, 2.0), 12)[1:-1]
    mouth = polygon(x, y, upper + lower)
    # One tooth hangs from the top lip, two rise from the bottom lip.
    tooth_top = polygon(x, y, [(-1.2, 2.0), (1.2, 2.0), (1.2, 5.8), (-1.2, 5.8)])
    tooth_bl = polygon(x, y, [(-6.4, 11.0), (-4.0, 11.0), (-4.0, 5.6), (-6.4, 5.6)])
    tooth_br = polygon(x, y, [(4.0, 11.0), (6.4, 11.0), (6.4, 5.6), (4.0, 5.6)])
    mouth = max(mouth, -tooth_top, -tooth_bl, -tooth_br)
    return min(eye_l, eye_r, nose, mouth)


def icon(kind, x, y):
    if kind == 12:
        return polygon(x, y, _BAT)
    if kind == 13:
        return _face(x, y)
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
    if kind == 5:
        return min(polygon(x,y,[(-8.5,-7),(3.5,-7),(3.5,-11),(9.5,-5),(3.5,1),(3.5,-3),(-8.5,-3)]),
                   polygon(x,y,[(8.5,3),(-3.5,3),(-3.5,-1),(-9.5,5),(-3.5,11),(-3.5,7),(8.5,7)]))
    if kind == 6:
        # Spiral polyline: only generated at build time, one atlas sample in GL.
        # Wound like the vortex arms: clockwise on screen when traced inward.
        points = [(math.cos(a) * (1.2 + a * 1.1), -math.sin(a) * (1.2 + a * 1.1))
                  for a in (i * math.tau * 1.35 / 48 for i in range(49))]
        return min(segment(x,y,a,b,1.35) for a,b in zip(points,points[1:]))
    # Tile 6 enabled for Whirlpool. Glyphs occupy 7..11.
    if kind == 7:
        return min(segment(x, y, (0, -10), (0, 3), 2.0), math.hypot(x, y-8)-2.0)
    if kind == 8:
        points = [(-6,-6),(-4,-10),(3,-10),(7,-6),(6,-2),(0,2),(0,4)]
        return min(min(segment(x,y,a,b,1.6) for a,b in zip(points,points[1:])), math.hypot(x,y-9)-1.8)
    if kind == 9:
        # One bowed L-bracket mirrored into four disconnected quadrants.
        x, y = abs(x), abs(y)
        return min(segment(x, y, a, b, 1.75) for a, b in zip(_ANGER, _ANGER[1:]))
    if kind == 10:
        points = [(-10,-7),(-2,-7),(-10,4),(-2,4)]
        little = [(2,0),(9,0),(2,8),(9,8)]
        return min(segment(x,y,a,b,1.1) for path in (points,little) for a,b in zip(path,path[1:]))
    if kind == 11:
        return min(math.hypot(x-4.2,y+3.5)-5.8, math.hypot(x+4.2,y+3.5)-5.8,
                   polygon(x,y,[(-9,-1),(9,-1),(0,11)]))
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

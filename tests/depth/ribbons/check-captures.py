#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Verify OLED stills, centred cover fit, and the unlit bezel in captured product frames."""
import argparse
import json
import math
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageStat


def frame(directory, second, role="world"):
    path = directory / f"{round(second * 1e9)}-{role}.png"
    with Image.open(path) as image:
        return image.convert("RGB")


def rmse(a, b, mask=None):
    channels = ImageStat.Stat(ImageChops.difference(a, b), mask).rms
    return math.sqrt(sum(channel ** 2 for channel in channels) / len(channels))


def lit_mask(image):
    # Measure against the local gradient. Dark pixels in the top and bottom
    # rows recover its endpoints even when a thin ribbon crosses the edge.
    w, h = image.size
    top = ImageStat.Stat(image.crop((0, 0, w, 1))).median
    bottom = ImageStat.Stat(image.crop((0, h-1, w, h))).median
    gradient = Image.new("RGB", (1, h))
    gradient.putdata([tuple(round(a+(b-a)*y/max(h-1, 1)) for a, b in zip(top, bottom)) for y in range(h)])
    delta = ImageChops.subtract(image, gradient.resize((w, h), Image.Resampling.NEAREST))
    r, g, b = delta.split()
    return ImageChops.lighter(r, ImageChops.lighter(g, b)).point(lambda v: 255 if v >= 16 else 0)


def balance(image):
    mask = lit_mask(image)
    w, h = mask.size
    cells = [mask.crop((x*w//3, y*h//3, (x+1)*w//3, (y+1)*h//3)).histogram()[255]
             for y in range(3) for x in range(3)]
    total = sum(cells)
    if total < 1000:
        raise ValueError("Insufficient silk to evaluate balance")
    shares = [v/total for v in cells]
    thirds = [sum(shares[3*y:3*y+3]) for y in range(3)] + [sum(shares[x::3]) for x in range(3)]
    return dict(thirds=thirds, maxCell=max(shares), litFraction=total/(w*h),
                passed=min(thirds)>=0.15 and max(thirds)<=0.50 and max(shares)<=0.35)


def check_support(candidate, support):
    image = frame(candidate / "scenes" / "A", 60)
    cover = Image.new("L", image.size)
    pen = ImageDraw.Draw(cover)
    bounds = support.read_text().splitlines()
    if not bounds:
        raise ValueError("Missing simulated ribbon bounds")
    for line in bounds:
        pen.rectangle(tuple(int(n) for n in line.split(",")), fill=255)
    mask = lit_mask(image)
    outside = ImageChops.subtract(mask, cover).histogram()[255]
    result = dict(segments=len(bounds), litPixels=mask.histogram()[255], outsideSupport=outside)
    if not result["litPixels"] or outside > 0:
        raise ValueError(f"Native frame exceeds simulated silk support {result}")
    return result


def compare_union(candidate, reference):
    measured = {}
    for path in sorted((reference/"scenes").glob("*/*-world.png")):
        relative = path.relative_to(reference)
        with Image.open(path) as a, Image.open(candidate/relative) as b:
            diff = ImageChops.difference(a.convert("RGB"), b.convert("RGB"))
        r, g, b = diff.split()
        hist = ImageChops.lighter(r, ImageChops.lighter(g, b)).histogram()
        measured[str(relative)] = dict(rms=ImageStat.Stat(diff).rms, pixelsAbove2=sum(hist[3:]),
                                       worst=max(i for i, n in enumerate(hist) if n))
    if not measured:
        raise ValueError("No corresponding native union captures")
    return measured


def check_folds(round_dir):
    scenes = round_dir / "scenes"
    # The screenshot is the 160 x 120 G/600 crop from the corrected full-union
    # reference. Allow isolated rasterization differences at native resolution.
    with Image.open(Path(__file__).with_name("g600-perpendicular.png")) as oracle:
        observed = frame(scenes / "G-fan", 600).crop((1052, 285, 1212, 405))
        r, g, b = ImageChops.difference(observed, oracle.convert("RGB")).split()
        histogram = ImageChops.lighter(r, ImageChops.lighter(g, b)).histogram()
    fan_pixels = sum(histogram[13:])
    if fan_pixels > 24:
        raise ValueError(f"G/600 bend differs from the Hermite-perpendicular reference at {fan_pixels} pixels")

    # Non-adjacent spans 19 and 45 on A/60 strand 3 cross near this pixel.
    # The native full-union oracle splits the strand at joint 32 to isolate
    # the sum of both contributions. The ordinary and split captures must
    # produce that same sum when the renderer is additive.
    expected = (78, 216, 255)
    colours = {scene: frame(scenes / scene, 60).getpixel((2214, 380))
               for scene in ("A-knot", "A-distinct-crossing")}
    if colours["A-knot"] != colours["A-distinct-crossing"]:
        raise ValueError(f"A/60 crossing changed with ribbon identity: {colours}")
    if max(abs(a-b) for a, b in zip(colours["A-knot"], expected)) > 3:
        raise ValueError(f"A/60 crossing RGB {colours['A-knot']}, expected additive {expected}")
    return dict(g600DifferingPixels=fan_pixels, a60CrossingRGB=colours)


def verify(round_dir):
    scenes = round_dir / "scenes"
    still = scenes / "F"
    first = frame(still, 0)
    for seconds in (60, 600):
        if ImageChops.difference(first, frame(still, seconds)).getbbox():
            raise ValueError(f"Frozen composition moved at {seconds} seconds")
    midpoint = frame(still, 604)
    new = frame(still, 608)
    r, g, b = ImageChops.difference(first, new).split()
    changed = ImageChops.lighter(r, ImageChops.lighter(g, b)).point(lambda v: 255 if v >= 15 else 0)
    changed_pixels = changed.histogram()[255]
    if changed_pixels < 1500:
        raise ValueError("Second composition did not replace the first")
    midpoint_rmse = rmse(midpoint, Image.blend(first, new, 0.5), changed)
    if midpoint_rmse > 3:
        raise ValueError(f"Crossfade midpoint RMSE {midpoint_rmse:.3f}")

    mirror = scenes / "D-mirrored"
    middle = frame(mirror, 60, "DP-5")
    left = frame(mirror, 60, "HDMI-A-5")
    x = (middle.width - left.width) // 2
    crop = middle.crop((x, 0, x + left.width, left.height))
    if max(high for _, high in crop.getextrema()) < 60:
        raise ValueError("Empty canonical cover crop")
    fit_rmse = rmse(crop, left)
    small = frame(mirror, 60, "DP-4")
    small_rmse = rmse(crop.resize(small.size, Image.Resampling.LANCZOS), small)
    if fit_rmse > 2 or small_rmse > 4:
        raise ValueError(f"Mirrored cover crop RMSE {fit_rmse:.3f}, {small_rmse:.3f}")

    gap = frame(scenes / "D-gap", 30)
    if gap.crop((2560, 0, 2620, gap.height)).getbbox():
        raise ValueError("Non-black pixels in the 60 px bezel")
    result = dict(frozenAtSeconds=[0, 60, 600], changedPixels=changed_pixels,
                  midpointRmse=midpoint_rmse, mirrorLeftRmse=fit_rmse,
                  mirrorSmallRmse=small_rmse, gapBrightPixels=0)
    if (scenes / "A-distinct-crossing").is_dir():
        result["foldRegressions"] = check_folds(round_dir)
    if (scenes / "Balance").is_dir():
        result["composition"] = {str(s): balance(frame(scenes / "Balance", s)) for s in range(5,301,5)}
    if (scenes / "G").is_dir():
        result["personalProfile"] = balance(frame(scenes / "G", 600))
    if (scenes / "D-spanning").is_dir():
        # Adjacent physical pixels on both sides of each seam carry light.
        panorama = frame(scenes / "D-spanning", 30)
        result["seamLitRows"] = {}
        for x, y, h in [(2560, 0, 1440), (6000, 180, 1080)]:
            strip = panorama.crop((x-1, y, x+1, y+h))
            result["seamLitRows"][str(x)] = sum(
                min(max(strip.getpixel((0,row))),max(strip.getpixel((1,row))))>65 for row in range(h))
        if (scenes / "D-offset").is_dir():
            changed = {}
            for role in ("HDMI-A-5", "DP-5", "DP-4"):
                difference = ImageChops.difference(frame(scenes / "D-spanning", 30, role),
                                                   frame(scenes / "D-offset", 30, role))
                r, g, b = difference.split()
                changed[role] = sum(ImageChops.lighter(r, ImageChops.lighter(g, b)).histogram()[1:])
            if changed["HDMI-A-5"] or changed["DP-4"] or not changed["DP-5"]:
                raise ValueError(f"Unexpected outputs changed under a +3 ms centre-panel offset: {changed}")
            result["offsetChangedPixels"] = changed
    return result



if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("round", type=Path)
    parser.add_argument("--union-reference", type=Path)
    parser.add_argument("--folds", action="store_true", help="Check the G/600 bend and A/60 strand identities")
    parser.add_argument("--support", type=Path, help="A/60 bounding rectangles from the native_scene_support Rust test")
    args = parser.parse_args()
    if args.folds:
        measurements = check_folds(args.round)
    elif args.support:
        measurements = check_support(args.round, args.support)
    elif args.union_reference:
        measurements = compare_union(args.round, args.union_reference)
    else:
        measurements = verify(args.round)
    print(json.dumps(measurements, indent=2))

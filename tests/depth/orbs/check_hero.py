#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Measure the default Midnight backdrop that is still visible on each output."""
import argparse
import json
from pathlib import Path

from PIL import Image, ImageChops


def dark_percent(path):
    with Image.open(path) as source:
        image = source.convert("RGB")

    # Midnight varies with height. Use the most frequent RGB value along
    # each row, where the uncovered background is a single repeated colour.
    samples = image.resize((min(320, image.width), image.height), Image.Resampling.NEAREST)
    row_colours = []
    for y in range(samples.height):
        strip = samples.crop((0, y, samples.width, y + 1))
        colours = strip.getcolors(strip.width)
        row_colours.append(max(colours, key=lambda value: value[0])[1])
    background = Image.new("RGB", (1, image.height))
    background.putdata(row_colours)
    delta = ImageChops.difference(image, background.resize(image.size, Image.Resampling.NEAREST))
    red, green, blue = delta.split()
    peak = ImageChops.lighter(ImageChops.lighter(red, green), blue)
    return 100 * sum(peak.histogram()[:6]) / (image.width * image.height)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("scene", type=Path, help="Directory of the C1 product capture")
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    manifest = json.loads((args.scene / "manifest.json").read_text())
    if manifest["scene"] != "C1" or manifest["visual"] != "orbs":
        parser.error("expected an Orbs C1 capture")
    roles = [output["role"] for output in manifest["outputs"]]
    times = sorted(int(frame["timelineNs"]) for frame in manifest["frames"])
    if len(set(roles)) != 3 or times != [10_000_000_000, 120_000_000_000, 900_000_000_000]:
        parser.error("expected all three C1 outputs at 10, 120 and 900 seconds")
    measurements = []
    for frame in manifest["frames"]:
        if sorted(frame["outputs"]) != sorted(f"{frame['timelineNs']}-{role}.png" for role in roles):
            parser.error("C1 output images are incomplete")
        for name in frame["outputs"]:
            percent = dark_percent(args.scene / name)
            measurements.append(dict(image=name, darkPercent=round(percent, 2), passed=55 <= percent <= 70))
    text = json.dumps(measurements, indent=2)
    print(text)
    if args.report:
        args.report.write_text(text + "\n")
    return 0 if all(row["passed"] for row in measurements) else 1


if __name__ == "__main__":
    raise SystemExit(main())

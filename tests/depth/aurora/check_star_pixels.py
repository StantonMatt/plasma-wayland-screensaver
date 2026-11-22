#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Check the two matched native Show stars captures from the private harness."""
import argparse
from pathlib import Path

from PIL import Image, ImageChops


def compare(directory):
    def read(scene):
        with Image.open(directory / "scenes" / scene / "80000000000-world.png") as image:
            return image.convert("RGB")

    on, off = read("starsOn"), read("starsOff")
    if on.size != off.size:
        raise ValueError("The captures must have the same dimensions")
    # Subtract in both directions with unsigned saturation. A decrease in
    # even one component of one pixel violates the emission contract.
    darker = ImageChops.subtract(off, on)
    brighter = ImageChops.subtract(on, off)
    for label, difference in (("darker", darker), ("brighter", brighter)):
        r, g, b = difference.split()
        histogram = ImageChops.lighter(ImageChops.lighter(r, g), b).histogram()
        print(f"{label}: {sum(histogram[1:])} pixels")
    return darker.getbbox() is None and brighter.getbbox() is not None


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("capture", type=Path)
    raise SystemExit(0 if compare(parser.parse_args().capture) else 1)

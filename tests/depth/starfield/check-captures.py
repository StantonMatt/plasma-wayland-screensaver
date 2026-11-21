#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Check the actual round's rendered pixels, still timer and settings pairs."""
import argparse
import hashlib
import json
from pathlib import Path

from PIL import Image, ImageChops, ImageFilter, ImageStat


def frames(scenes, name):
    folder = scenes / name
    manifest = json.loads((folder / "manifest.json").read_text())
    return manifest, [folder / frame["panorama"] for frame in manifest["frames"]]


def pixel_hash(path):
    with Image.open(path) as im:
        return hashlib.sha256(im.convert("RGB").tobytes()).hexdigest()


def distribution(path):
    def linear(c):
        s = c / 255
        return round(255 * (s / 12.92 if s <= 0.04045 else ((s + 0.055) / 1.055) ** 2.4))
    with Image.open(path) as im:
        rgb = im.convert("RGB").point([linear(c) for c in range(256)] * 3)
        hist = rgb.convert("L", matrix=(0.2126, 0.7152, 0.0722, 0)).histogram()
        # Low-frequency gas, the galaxy band and a single bright landmark can
        # all survive a broken star instance stream. Count spatially separated
        # highlights over their local background as evidence of resolved stars.
        gray = im.convert("L")
        details = ImageChops.subtract(gray, gray.filter(ImageFilter.GaussianBlur(2)))
        lit_tiles = sum(details.crop((x, y, x + 64, y + 64)).getextrema()[1] >= 35
                        for y in range(0, im.height, 64) for x in range(0, im.width, 64))
        return dict(lit_tiles=lit_tiles, dark_fraction=sum(hist[:13]) / sum(hist), bright_pixels=sum(hist[26:]))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("round", type=Path)
    args = parser.parse_args()
    scenes = args.round / "scenes"
    manifest, still = frames(scenes, "S1b")
    assert manifest["timerStoppedAtActivation"] and pixel_hash(still[0]) == pixel_hash(still[1])
    _, cruise = frames(scenes, "S1")
    assert pixel_hash(cruise[0]) != pixel_hash(cruise[1])
    light = distribution(cruise[0])
    assert light["dark_fraction"] > 0.84 and light["bright_pixels"] > 100 and light["lit_tiles"] >= 100, light
    colors = ("ocean", "spectrum", "ember", "forest", "mono", "pastel")
    assert len({pixel_hash(frames(scenes, f"S3-{name}")[1][0]) for name in colors}) == 6
    for a, b in [("events-false", "events-true"), ("steering-0", "steering-100"), ("nebulae-0", "nebulae-100"),
                 ("flight-cruise", "flight-hyperspace"), ("speed-10", "speed-300"), ("density-10", "density-100"),
                 ("scale-25", "scale-200"), ("trails-0", "trails-100")]:
        assert pixel_hash(frames(scenes, a)[1][0]) != pixel_hash(frames(scenes, b)[1][0]), (a, b)
    span, panoramas = frames(scenes, "S5")
    assert span["worldCount"] == 1 and len(span["outputs"]) == 3
    for frame, path in zip(span["frames"], panoramas):
        with Image.open(path) as world:
            for output in span["outputs"]:
                x, y, w, h = [output[k] for k in ("x", "y", "width", "height")]
                name = f"{frame['timelineNs']}-{output['role']}.png"
                with Image.open(path.parent / name) as native:
                    assert ImageChops.difference(native.convert("RGB"), world.crop((x, y, x + w, y + h)).convert("RGB")).getbbox() is None
    _, gas = frames(scenes, "S4")
    _, control = frames(scenes, "S4-gas-control")
    gas_area = []
    for cloud, clean in zip(gas, control, strict=True):
        with Image.open(cloud) as a, Image.open(clean) as b:
            red = ImageChops.subtract(a.getchannel("R"), b.getchannel("R")).filter(ImageFilter.GaussianBlur(4))
            histogram = red.histogram()
            # A blurred 8-bit value of ten spans 9.5–10.5. Split that
            # histogram bin at the exact ten-level visibility threshold.
            fraction = (sum(histogram[11:]) + 0.5 * histogram[10]) / sum(histogram)
            assert 0.30 <= fraction <= 0.60, f"Gas emission covers {fraction:.1%} in {cloud}"
            gas_area.append(round(fraction, 4))
    with Image.open(frames(scenes, "G")[1][0]) as source:
        # Leave clipped heads outside the sample. Resolve local peaks over the
        # gas so the background gradient cannot satisfy the colour check.
        image = source.convert("RGB").crop((5, 5, source.width - 5, source.height - 5))
        gray = image.convert("L")
        high = ImageChops.subtract(gray, gray.filter(ImageFilter.GaussianBlur(3)))
        peaks = ImageChops.difference(gray, gray.filter(ImageFilter.MaxFilter(5))).point(lambda d: 255 if d == 0 else 0)
        cores = ImageChops.multiply(peaks, high.point(lambda v: 255 if v >= 25 else 0))
        samples = [image.getpixel((i % image.width, i // image.width)) for i, v in enumerate(cores.tobytes()) if v]
        bright = [(r, g, b) for r, g, b in samples if r >= 160]
        assert len(bright) > 50 and all(g >= b for _, g, b in bright), "Ember cores turned pink"
    with Image.open(frames(scenes, "trails-0")[1][0]) as a, Image.open(frames(scenes, "trails-100")[1][0]) as b:
        trails_delta = sum(ImageStat.Stat(ImageChops.difference(a.convert("RGB"), b.convert("RGB"))).mean) / 3
        assert trails_delta > 0.5, "Trails control produced almost identical native images"
    _, phases = frames(scenes, "S2")
    assert len({pixel_hash(p) for p in phases}) == 5
    fallback, fallback_frames = frames(scenes, "fallback")
    assert fallback["worldCount"] == 0 and fallback["legacyBrightPixels"] >= fallback["fallbackPixels"]["minimum"]
    with Image.open(fallback_frames[0]) as image:
        plain = image.convert("L")
        y = image.height // 2
        def middle_patch(x):
            return ImageStat.Stat(plain.crop((x - 32, y - 32, x + 32, y + 32))).median[0]
        fallback_centre_excess = middle_patch(image.width // 2) - middle_patch(image.width // 5)
        assert abs(fallback_centre_excess) <= 5, f"Software fog disc: {fallback_centre_excess} levels"
    for name, (count, fps) in {"recording": (5400, 60), "jump-30": (600, 30),
                               "G-motion": (1200, 60), "steering-span": (1800, 60)}.items():
        recorded, _ = frames(scenes, name)
        assert recorded["videoFrames"] == count and recorded["frameCap"] == fps
        assert recorded["video"]["fps"] == fps and recorded["recordings"]
    mirrored, _ = frames(scenes, "mirrored")
    assert mirrored["worldCount"] == 1 and len(mirrored["outputs"]) == 2
    report = dict(default_cruise=light, still_sha256=pixel_hash(still[0]), still_timer_stopped=True,
                  settings_pairs=8, distinct_palettes=6, stitched_native_frames=len(panoramas),
                  fallback_centre_excess=fallback_centre_excess, recording_scenes=4, gas_area=gas_area,
                  ember_mid_bright_samples=len(bright), trails_mean_delta=round(trails_delta, 3))
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()

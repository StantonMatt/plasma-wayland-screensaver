#!/usr/bin/python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Check the historical head-contour diagnostic above its RGB8 resolution limit."""
import importlib.util
import math
from pathlib import Path
import tempfile
import unittest

from PIL import Image

spec = importlib.util.spec_from_file_location('head_check', Path(__file__).with_name('check-heads.py'))
head_check = importlib.util.module_from_spec(spec)
spec.loader.exec_module(head_check)


class HaloResolution(unittest.TestCase):
    def measure(self, level, width, halo=False):
        radius = 16. if halo else 3.5
        image = Image.new('RGB', (128, 64))
        if halo:
            for y in range(64):
                for x in range(64):
                    value = round(level * math.exp(-4.7 * math.hypot(x-32, y-32)/radius))
                    image.putpixel((x, y), (value, value, value))
        else:
            image.putpixel((32, 32), (level, 0, 0))
        image.putpixel((96, 32), (255, 255, 255))
        section = [0., 0., 0., 1., width/2, 1., radius, 1., 0.]
        meta = dict(x=32.5, y=32.5, radius=radius, fx=-1., fy=0.,
                    maskOffset=64, fraction=0., sections=[[section, section]])
        with tempfile.TemporaryDirectory(prefix='ribbon-head-check-') as directory:
            path = Path(directory) / 'head.png'
            image.save(path)
            return head_check.sample(path, meta)

    def test_faded_subpixel_lens_has_no_measurable_contour(self):
        result = self.measure(1, .94)
        self.assertFalse(result['extentResolved'])
        self.assertEqual(result['photometricHalfExtent'], 0.)
        self.assertLess(result['nativePeak'], 1.)
        self.assertTrue(result['passed'])

    def test_bright_cut_off_subpixel_head_still_fails(self):
        result = self.measure(255, .94)
        self.assertTrue(result['extentResolved'])
        self.assertEqual(result['maxRise'], 0.)
        self.assertEqual(result['maxAdjacent'], 0.)
        self.assertEqual(result['photometricHalfExtent'], 0.)
        self.assertFalse(result['passed'])

    def test_blank_mature_head_is_not_excused_as_unresolved(self):
        result = self.measure(0, 16.)
        self.assertTrue(result['extentResolved'])
        self.assertFalse(result['passed'])

    def test_resolved_smooth_halo_passes_all_original_limits(self):
        result = self.measure(5, 16., halo=True)
        self.assertTrue(result['extentResolved'])
        self.assertGreaterEqual(result['photometricHalfExtent'], .6)
        self.assertTrue(result['passed'])


if __name__ == '__main__':
    unittest.main()

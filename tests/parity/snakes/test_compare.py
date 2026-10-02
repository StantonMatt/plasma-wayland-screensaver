#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Check that the parity policy rejects missed discrete and numerical changes."""
import copy
import unittest

from compare import compare_fixture


class ComparisonPolicy(unittest.TestCase):
    def setUp(self):
        self.frame = {
            'tick': 310, 'rngState': 42, 'rngDraws': 100,
            'snakes': [{'alive': True, 'length': 20, 'segments': [[1.0, 2.0]]}],
            'food': [[83, 1.0, 2.0, 3.0, -1, 0.0]],
            'events': [{'type': 'death', 'tick': 305, 'snake': 1,
                        'reason': 'head', 'killer': 0, 'killerCandidates': [0],
                        'emittedFood': 20}],
        }

    def compare(self, change):
        actual = copy.deepcopy(self.frame)
        change(actual)
        return compare_fixture({'frames': [self.frame]}, [actual])

    def test_rounding_tolerance(self):
        errors, maximum, early, _, divergence = self.compare(
            lambda f: f['food'][0].__setitem__(1, 1.0000005))
        self.assertFalse(errors)
        self.assertAlmostEqual(maximum, 5e-7)
        self.assertEqual(early, 0)
        self.assertIsNone(divergence)

    def test_food_ids_and_owners_are_exact(self):
        for column in (0, 4):
            with self.subTest(column=column):
                errors, *_ = self.compare(lambda f: f['food'][0].__setitem__(
                    column, f['food'][0][column]+1e-7))
                self.assertTrue(errors)

    def test_rng_and_lengths_are_exact(self):
        for key in ('rngState', 'rngDraws'):
            with self.subTest(key=key):
                errors, *_ = self.compare(lambda f: f.__setitem__(key, f[key]+1))
                self.assertTrue(errors)
        errors, *_ = self.compare(lambda f: f['snakes'][0].__setitem__('length', 21))
        self.assertTrue(errors)

    def test_unsampled_event_tick_and_reason_are_exact(self):
        for key, value in (('tick', 306), ('reason', 'body'), ('snake', 0),
                           ('killer', 2), ('emittedFood', 21)):
            with self.subTest(key=key):
                errors, *_ = self.compare(lambda f: f['events'][0].__setitem__(key, value))
                self.assertTrue(errors)

    def test_late_divergence_is_not_silently_accepted(self):
        errors, _, _, _, divergence = self.compare(
            lambda f: f['snakes'][0]['segments'][0].__setitem__(0, 1.000002))
        self.assertTrue(errors)
        self.assertEqual(divergence, 310)

    def test_nonfinite_output_fails(self):
        errors, *_ = self.compare(lambda f: f['food'][0].__setitem__(1, float('nan')))
        self.assertTrue(errors)


if __name__ == '__main__':
    unittest.main()

#!/usr/bin/env python3
"""Pure request checks; never opens a compositor or display."""
import importlib.util
from pathlib import Path
from types import SimpleNamespace
import unittest

spec = importlib.util.spec_from_file_location('depth_capture', Path(__file__).with_name('run-depth-capture.py'))
capture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(capture)


class SceneOverridesTest(unittest.TestCase):
    def request(self, **overrides):
        args = SimpleNamespace(catalog={'defaults': {'speed': 100, 'points': 8}, 'scenes': {
            'G': {'layout': 'l3', 'seed': '7', 'settings': {'speed': 300, 'points': 14},
                  'events': [{'id': 5, 'timelineNs': '42000000000'}]}}},
            visual='kaleidoscope', event_script=None, refresh_rates=None,
            scene_seed=None, scene_speed=None, scene_defaults=False)
        for key, value in overrides.items():
            setattr(args, key, value)
        return capture.scene_request(args, 'G')

    def test_existing_scene_is_preserved(self):
        request = self.request()
        self.assertEqual(request['seed'], '7')
        self.assertEqual(request['settings'], {'speed': 300, 'points': 14})

    def test_matrix_overrides_preserve_events_and_layout(self):
        request = self.request(scene_seed='23', scene_speed=100)
        self.assertEqual(request['seed'], '23')
        self.assertEqual(request['settings'], {'speed': 100, 'points': 14})
        self.assertEqual(request['events'], self.request()['events'])
        self.assertEqual(request['outputs'], self.request()['outputs'])

    def test_defaults_are_copied_before_speed_override(self):
        request = self.request(scene_defaults=True, scene_speed=300)
        self.assertEqual(request['settings'], {'speed': 300, 'points': 8})
        self.assertEqual(self.request()['settings']['points'], 14)


if __name__ == '__main__':
    unittest.main()

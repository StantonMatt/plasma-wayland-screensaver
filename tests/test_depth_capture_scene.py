#!/usr/bin/env python3
"""Pure request checks; never opens a compositor or display."""
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

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


class CaptureExitStatusTest(unittest.TestCase):
    def run_capture(self, capture_code=0, checker_code=0, checker_error=None):
        with tempfile.TemporaryDirectory(prefix='depth-capture-status-') as temporary:
            root = Path(temporary)
            binary = root / 'bin/test-visual-depth-capture'
            binary.parent.mkdir()
            binary.write_bytes(b'capture test binary')
            output = root / 'output'

            def captured(**kwargs):
                scene = output / 'scenes/Recording'
                scene.mkdir(parents=True)
                (scene / 'manifest.json').write_text(json.dumps(dict(videoFrames=60)))
                return capture_code

            def checked(*args, **kwargs):
                if checker_error:
                    raise checker_error
                (output / 'luminance.json').write_text(json.dumps(dict(passed=not checker_code)))
                return subprocess.CompletedProcess(args[0], checker_code)

            process = Mock(pid=12345)
            process.wait.side_effect = captured
            process.poll.return_value = capture_code
            argv = [str(capture.__file__), '--build', str(root), '--visual', 'fireflies',
                    '--scene', 'Recording', '--output', str(output)]
            with patch.object(sys, 'argv', argv), \
                    patch.object(capture.subprocess, 'check_output', side_effect=['test-head', b'', b'']), \
                    patch.object(capture.subprocess, 'Popen', return_value=process), \
                    patch.object(capture.subprocess, 'run', side_effect=checked) as checker, \
                    patch.object(capture.os, 'killpg'):
                code = capture.main()
            manifest = output / 'manifest.json'
            return code, json.loads(manifest.read_text()) if manifest.exists() else None, checker.call_count

    def test_failed_luminance_is_recorded_but_capture_succeeds(self):
        code, manifest, calls = self.run_capture(checker_code=1)
        self.assertEqual(code, 0)
        self.assertEqual(calls, 1)
        self.assertEqual(manifest['luminance'], dict(report='luminance.json', passed=False, exitCode=1))

    def test_checker_errors_are_diagnostic(self):
        for error in [OSError('checker unavailable'), subprocess.TimeoutExpired('checker', 3600)]:
            with self.subTest(error=error):
                code, manifest, _ = self.run_capture(checker_error=error)
                self.assertEqual(code, 0)
                self.assertFalse(manifest['luminance']['passed'])
                self.assertIn('error', manifest['luminance'])

    def test_passing_luminance_is_recorded(self):
        code, manifest, _ = self.run_capture()
        self.assertEqual(code, 0)
        self.assertTrue(manifest['luminance']['passed'])

    def test_capture_failure_retains_its_exit_code(self):
        code, manifest, calls = self.run_capture(capture_code=7)
        self.assertEqual(code, 7)
        self.assertIsNone(manifest)
        self.assertEqual(calls, 0)


if __name__ == '__main__':
    unittest.main()

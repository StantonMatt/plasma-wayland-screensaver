#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Exercise CLI discovery and forced-event validation without a compositor."""
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

runner_path = Path(__file__).resolve().parents[1] / 'run-depth-capture.py'
spec = importlib.util.spec_from_file_location('depth_capture', runner_path)
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class SceneCatalogTest(unittest.TestCase):
    def test_new_visual_and_sparse_event_ids(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            catalog = root / 'scenes/example/round.json'
            catalog.parent.mkdir(parents=True)
            scene = dict(layout='single', monitorMode='independent', seed='9', captureTimesNs=['0'])
            catalog.write_text(json.dumps(dict(eventIds=[0, 3, 42, 2**32 - 1], defaults={}, scenes={'A': scene})))
            script = root / 'events.json'
            argv = [str(runner_path), '--worker', '--visual', 'example', '--scene', 'A',
                    '--build', directory, '--output', str(root / 'out'), '--event-script', str(script)]
            # Limit discovery to the temporary catalog while running the real parser.
            catalogs = runner.scene_catalogs(root / 'scenes')
            with patch.object(sys, 'argv', argv), patch.object(runner, 'scene_catalogs', return_value=catalogs), \
                    patch.object(runner, 'worker', return_value=0) as worker:
                event = dict(id=42, timelineNs=8, anchor=[1, 2], strength=0.25)
                events = [dict(event, id=i) for i in (0, 42, 2**32 - 1)]
                script.write_text(json.dumps(events))
                self.assertEqual(runner.main(), 0)
                args = worker.call_args.args[0]
                request = runner.scene_request(args, 'A')
                self.assertEqual((request['visual'], request['events']),
                                 ('example', [dict(e, timelineNs='8') for e in events]))
                args.catalog['scenes']['legacy'] = dict(scene, legacy=True)
                self.assertEqual(runner.scene_request(args, 'legacy')['events'], [])
                script.write_text(json.dumps([dict(event, id=4)]))
                with contextlib.redirect_stderr(io.StringIO()) as errors, self.assertRaises(SystemExit):
                    runner.main()
                self.assertIn('unknown event ID', errors.getvalue())
                worker.assert_called_once()


if __name__ == '__main__':
    unittest.main()

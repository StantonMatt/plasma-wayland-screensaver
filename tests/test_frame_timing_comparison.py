# SPDX-License-Identifier: GPL-3.0-or-later
"""Regression and failure-path checks for the release-relative CI cadence gate."""
import json
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import tempfile
import unittest


SCRIPTS = Path(__file__).resolve().parents[1] / "scripts"
COMPARISON = runpy.run_path(str(SCRIPTS / "compare-frame-timing.py"))


def summary(frames=(1200, 1200, 1200)):
    return {"duration_requested_s": 30, "warmup_excluded_s": 5, "outputs": 3,
            "output_size": "640x360", "configured_fps": "60", "graphics_api_requested": "vulkan",
            "validation": {"checked": True, "failures": []},
            "windows": {str(window): {"frames": count, "frame_interval_ms": {"p50": 16.6667},
                        "cadence": {"advertised_refresh_hz": 60}}
                        for window, count in enumerate(frames, 1)}}


class ComparisonTest(unittest.TestCase):
    def compare(self, baseline=None, candidate=None):
        return COMPARISON["compare"](baseline if baseline is not None else [summary(), summary()],
                                     candidate if candidate is not None else [summary(), summary()])

    def test_identical_builds(self):
        result = self.compare()
        self.assertEqual(result["failures"], [])
        self.assertEqual([row["candidate_baseline_ratio"] for row in result["windows"]], [1, 1, 1])

    def test_ten_percent_boundary_passes(self):
        self.assertEqual(self.compare(candidate=[summary((1080,) * 3)] * 2)["failures"], [])

    def test_one_window_regression_fails(self):
        result = self.compare(candidate=[summary((1200, 1079, 1200))] * 2)
        self.assertEqual(len(result["failures"]), 1)
        self.assertIn("window 2", result["failures"][0])

    def test_balanced_run_means_absorb_linear_drift(self):
        result = self.compare([summary((1200,) * 3), summary((1440,) * 3)],
                              [summary((1280,) * 3), summary((1360,) * 3)])
        self.assertEqual(result["failures"], [])
        self.assertEqual(result["windows"][0]["candidate_baseline_ratio"], 1)

    def test_individual_stall_cannot_hide_in_average(self):
        result = self.compare(candidate=[summary((1049, 1200, 1200)), summary((1500,) * 3)])
        self.assertIn("candidate 1 window 1", result["failures"][0])

    def test_absolute_fallback_and_floor_boundary(self):
        result = self.compare(baseline=[], candidate=[summary((1050,) * 3)] * 2)
        self.assertEqual(result["mode"], "absolute fallback")
        self.assertEqual(result["failures"], [])

    def test_median_failure_even_with_enough_frames(self):
        candidate = summary()
        candidate["windows"]["2"]["frame_interval_ms"]["p50"] = 22
        self.assertIn("median", self.compare(candidate=[candidate, summary()])["failures"][0])

    def test_backend_or_policy_failure_is_preserved(self):
        candidate = summary()
        candidate["validation"]["failures"] = ["Requested vulkan was not selected"]
        self.assertIn("Requested vulkan", self.compare(candidate=[candidate])["failures"][0])

    def test_incomparable_settings_rejected(self):
        for key, value in (("duration_requested_s", 45), ("warmup_excluded_s", 0),
                           ("output_size", "800x600"), ("graphics_api_requested", "opengl")):
            with self.subTest(key=key):
                baseline = summary()
                baseline[key] = value
                with self.assertRaises(ValueError):
                    self.compare(baseline=[baseline])
        baseline = summary()
        baseline["windows"]["1"]["cadence"]["advertised_refresh_hz"] = 120
        with self.assertRaises(ValueError):
            self.compare(baseline=[baseline])

    def test_input_validation_and_cli_failure_json(self):
        with tempfile.TemporaryDirectory(prefix="pvs-comparison-test-") as temporary:
            root = Path(temporary)
            path = root / "summary.json"
            path.write_text(json.dumps(summary()))
            self.assertEqual(COMPARISON["load"](path), summary())
            for key, value in (("outputs", 4), ("warmup_excluded_s", 30), ("duration_requested_s", float("nan"))):
                report = summary()
                report[key] = value
                path.write_text(json.dumps(report))
                with self.assertRaises(ValueError):
                    COMPARISON["load"](path)
            for value in (0, float("inf"), True):
                report = summary()
                report["windows"]["1"]["frames"] = value
                path.write_text(json.dumps(report))
                with self.assertRaises(ValueError):
                    COMPARISON["load"](path)
            report = summary()
            report["validation"]["checked"] = False
            path.write_text(json.dumps(report))
            with self.assertRaises(ValueError):
                COMPARISON["load"](path)
            baseline = root / "baseline.json"
            baseline.write_text(json.dumps(summary()))
            path.write_text(json.dumps(summary((1079, 1200, 1200))))
            result = subprocess.run(["python3", str(SCRIPTS / "compare-frame-timing.py"),
                                     "--candidate", str(path), "--baseline", str(baseline),
                                     "--output", str(root / "comparison.json")], capture_output=True, text=True, timeout=10)
            self.assertEqual(result.returncode, 1)
            self.assertIn("Window |", result.stdout)
            self.assertIn("89.92%", result.stdout)
            self.assertTrue(json.loads((root / "comparison.json").read_text())["failures"])


class InterleavingTest(unittest.TestCase):
    def run_gate(self, baseline="release", failed_run="", missing_summary=False):
        with tempfile.TemporaryDirectory(prefix="pvs-interleave-test-") as temporary:
            root = Path(temporary)
            for name in ("ci-frame-cadence.sh", "compare-frame-timing.py"):
                shutil.copy2(SCRIPTS / name, root / name)
            # Exercise the actual runner/comparison while substituting only the expensive compositor.
            harness = root / "frame-timing.sh"
            harness.write_text('''#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
args = sys.argv[1:]
directory = Path(args[args.index('--output-dir') + 1])
binary = args[args.index('--binary') + 1]
with (directory.parent / 'order.txt').open('a') as stream:
    stream.write(binary + '\\n')
assert args[args.index('--duration') + 1] == '30'
assert args[args.index('--warmup') + 1] == '5'
assert args[args.index('--min-frame-ratio') + 1] == '0.7'
assert args[args.index('--render-loop') + 1] == 'basic'
assert '--check' in args
failed = directory.name == os.environ['FAILED_RUN']
if not (failed and os.environ['MISSING_SUMMARY'] == '1'):
    directory = directory / '3output-60fps'
    directory.mkdir(parents=True)
    report = json.loads(os.environ['REPORT'])
    if failed:
        report['validation']['failures'] = ['injected failure']
    (directory / 'summary.json').write_text(json.dumps(report))
sys.exit(1 if failed else 0)
''')
            harness.chmod(0o755)
            result = subprocess.run([str(root / "ci-frame-cadence.sh"), "candidate", baseline,
                                     "vulkan", str(root / "results")], capture_output=True, text=True, timeout=10,
                                    env=dict(os.environ, REPORT=json.dumps(summary()), FAILED_RUN=failed_run,
                                             MISSING_SUMMARY=str(int(missing_summary))))
            order = (root / "results/order.txt").read_text().splitlines()
            comparison = root / "results/comparison.json"
            return result, order, json.loads(comparison.read_text()) if comparison.exists() else None

    def test_baab(self):
        result, order, report = self.run_gate()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(order, ["release", "candidate", "candidate", "release"])
        self.assertEqual(report["mode"], "relative")

    def test_missing_release(self):
        result, order, report = self.run_gate(baseline="")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(order, ["candidate", "candidate"])
        self.assertIn("::warning::", result.stdout)
        self.assertEqual(report["mode"], "absolute fallback")

    def test_bad_release_trace_falls_back(self):
        for failed in ("baseline-1", "baseline-2"):
            with self.subTest(failed=failed):
                result, _, report = self.run_gate(failed_run=failed, missing_summary=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn("::warning::", result.stdout)
                self.assertEqual(report["mode"], "absolute fallback")

    def test_candidate_failure_cannot_be_masked_by_fallback(self):
        result, _, report = self.run_gate(baseline="", failed_run="candidate-1")
        self.assertEqual(result.returncode, 1)
        self.assertTrue(report["failures"])

    def test_candidate_missing_trace_fails_and_runs_finish(self):
        result, order, report = self.run_gate(failed_run="candidate-1", missing_summary=True)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(order, ["release", "candidate", "candidate", "release"])
        self.assertIsNone(report)


if __name__ == "__main__":
    unittest.main()

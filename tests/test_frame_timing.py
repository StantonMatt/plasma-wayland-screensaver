# SPDX-License-Identifier: GPL-3.0-or-later
"""Exercise the CI cadence gate using traces with known failures."""
import csv
import json
import math
from pathlib import Path
import subprocess
import tempfile
import unittest


HARNESS = Path(__file__).resolve().parents[1] / "scripts/frame-timing.sh"


class FrameTimingTest(unittest.TestCase):
    def run_trace(self, counts=(240, 240, 240), intervals=None,
                  refreshes=(60, 60, 60), backend="vulkan", empty=False,
                  presentation="render thread true swap interval 0", requested="vulkan", target=60):
        if intervals is None:
            intervals = tuple(round(1e9 / (r / max(1, math.ceil(r / 60)))) for r in refreshes)
        with tempfile.TemporaryDirectory(prefix="pvs-timing-test-") as temporary:
            root = Path(temporary)
            directory = root / "3output-60fps"
            directory.mkdir()
            (directory / "app.log").write_text(
                f"PVS graphics backend: {backend}\n"
                f"Creating QRhi with backend {'Vulkan' if backend == 'vulkan' else 'OpenGL'}\n"
                + "".join(f"Frame trace window {w} target FPS {target} refresh Hz {refresh}\n"
                          f"Frame trace initialized window {w} graphics API {backend} {presentation}\n"
                          for w, refresh in enumerate(refreshes, 1)))
            fields = ("kind", "window", "timestamp_ns", "interval_ns", "gui_tick_ns",
                      "sync_ns", "render_ns", "simulation_steps", "tick_callbacks",
                      "rss_bytes", "cpu_ns")
            with (directory / "frames.csv").open("w") as stream:
                writer = csv.DictWriter(stream, fieldnames=fields)
                writer.writeheader()
                for timestamp in (1_000_000_000, 5_000_000_000):
                    writer.writerow(dict.fromkeys(fields, 0) | {
                        "kind": "resource", "timestamp_ns": timestamp})
                if not empty:
                    for window, (count, interval) in enumerate(zip(counts, intervals), 1):
                        for frame in range(count):
                            writer.writerow(dict.fromkeys(fields, 0) | {
                                "kind": "frame", "window": window,
                                "timestamp_ns": 1_000_000_000 + frame * interval,
                                "interval_ns": interval if frame else 0})
            result = subprocess.run([str(HARNESS), "--summarize-only", "--outputs", "3",
                "--rates", "60", "--duration", "4", "--warmup", "0", "--check",
                "--graphics-api", requested, "--output-dir", str(root)],
                capture_output=True, text=True, timeout=10)
            summary = directory / "summary.json"
            return result, json.loads(summary.read_text()) if summary.exists() else None

    def test_healthy_three_outputs(self):
        result, report = self.run_trace()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(report["validation"]["failures"], [])

    def test_even_divisors_for_mixed_refresh(self):
        result, _ = self.run_trace(counts=(200, 240, 233), refreshes=(100, 240, 175))
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_opengl_control_keeps_its_presentation_policy(self):
        result, _ = self.run_trace(backend="opengl", requested="opengl",
                                  presentation="render thread false swap interval 1")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_configuration_was_ignored(self):
        result, report = self.run_trace(target=30)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("target FPS 30 != configured 60", report["validation"]["failures"][0])

    def test_one_window_stops_early(self):
        result, report = self.run_trace(counts=(240, 240, 100))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Window 3: 100 frames < 80%", report["validation"]["failures"][0])

    def test_wrong_median_even_with_enough_frames(self):
        result, report = self.run_trace(intervals=(16_666_667, 16_666_667, 10_000_000))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("differs >25%", report["validation"]["failures"][0])

    def test_backend_fallback(self):
        result, report = self.run_trace(backend="opengl")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("was not selected", report["validation"]["failures"][0])

    def test_gui_thread_fifo_vulkan(self):
        result, report = self.run_trace(presentation="render thread false swap interval 1")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("requires a render thread and swap interval 0", report["validation"]["failures"][0])

    def test_missing_window(self):
        result, _ = self.run_trace(counts=(240, 240, 0))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Expected 3 output windows, got 2", result.stderr)

    def test_total_freeze(self):
        result, _ = self.run_trace(empty=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Trace lacks frames", result.stderr)


if __name__ == "__main__":
    unittest.main()

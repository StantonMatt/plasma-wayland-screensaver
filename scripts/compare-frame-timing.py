#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Compare interleaved release/candidate virtual-Wayland submission traces."""
import argparse
import json
import math
from pathlib import Path
import statistics
import sys


def positive(value, label):
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or value <= 0:
        raise ValueError(f"{label}: expected a finite positive number")
    return value


def load(path):
    report = json.loads(path.read_text())
    duration = positive(report["duration_requested_s"], "duration")
    warmup = report["warmup_excluded_s"]
    if not isinstance(warmup, (int, float)) or not math.isfinite(warmup) or not 0 <= warmup < duration:
        raise ValueError("warmup must be finite and less than duration")
    windows = report["windows"]
    if not windows or len(windows) != report["outputs"]:
        raise ValueError("output count does not match windows")
    for window, metrics in windows.items():
        positive(metrics["frames"], f"window {window} frames")
        positive(metrics["frame_interval_ms"]["p50"], f"window {window} median interval")
        positive(metrics["cadence"]["advertised_refresh_hz"], f"window {window} refresh")
    if report["validation"]["checked"] is not True:
        raise ValueError("summary must be collected with --check")
    return report


def compare(baselines, candidates, max_regression=.10, min_frame_ratio=.70):
    failures = []
    reference = candidates[0]
    keys = ("duration_requested_s", "warmup_excluded_s", "outputs", "output_size",
            "configured_fps", "graphics_api_requested")
    for label, reports in (("baseline", baselines), ("candidate", candidates)):
        for index, report in enumerate(reports, 1):
            if any(report[key] != reference[key] for key in keys) or report["windows"].keys() != reference["windows"].keys():
                raise ValueError(f"{label} {index}: measurement settings/windows differ")
            for window, metrics in report["windows"].items():
                if metrics["cadence"]["advertised_refresh_hz"] != reference["windows"][window]["cadence"]["advertised_refresh_hz"]:
                    raise ValueError(f"{label} {index}: window {window} refresh differs")
            failures.extend(f"{label} {index}: {failure}" for failure in report["validation"]["failures"])

    rows = []
    duration = reference["duration_requested_s"] - reference["warmup_excluded_s"]
    target = 60 if reference["configured_fps"] == "auto" else int(reference["configured_fps"])
    positive(target, "configured FPS")
    for window in sorted(reference["windows"], key=int):
        refresh = reference["windows"][window]["cadence"]["advertised_refresh_hz"]
        expected_hz = refresh / max(1, math.ceil(refresh / target))
        expected_interval = 1000 / expected_hz
        candidate_frames = [report["windows"][window]["frames"] for report in candidates]
        baseline_frames = [report["windows"][window]["frames"] for report in baselines]
        medians = [report["windows"][window]["frame_interval_ms"]["p50"] for report in candidates]
        for index, (frames, median) in enumerate(zip(candidate_frames, medians), 1):
            if frames < min_frame_ratio * expected_hz * duration:
                failures.append(f"candidate {index} window {window}: {frames} frames below {min_frame_ratio:.0%} stall floor")
            if abs(median / expected_interval - 1) > .25:
                failures.append(f"candidate {index} window {window}: median {median:.4f}ms differs >25% from {expected_interval:.4f}ms")
        baseline_mean = statistics.mean(baseline_frames) if baselines else None
        candidate_mean = statistics.mean(candidate_frames)
        ratio = candidate_mean / baseline_mean if baselines else None
        if ratio is not None and ratio < 1 - max_regression:
            failures.append(f"window {window}: candidate/baseline {ratio:.2%} below {1 - max_regression:.0%}")
        rows.append({"window": window, "baseline_frames": baseline_frames,
                     "candidate_frames": candidate_frames, "baseline_mean_frames": baseline_mean,
                     "candidate_mean_frames": candidate_mean, "candidate_baseline_ratio": ratio,
                     "candidate_median_interval_ms": medians})
    return {"mode": "relative" if baselines else "absolute fallback", "max_regression": max_regression,
            "min_frame_ratio": min_frame_ratio, "windows": rows, "failures": failures}


def main():
    parser = argparse.ArgumentParser(description=__doc__, epilog=(
        "Use two 30s runs per build, warmup 5s, ordered baseline/candidate/candidate/baseline. "
        "Equal settings and checked summaries are required. Compare mean frame counts per window; "
        "each candidate also needs >=70% of expected frames and median interval within 25% of target. "
        "Omit --baseline for the absolute fallback. These are Qt submissions, not scanout FPS."))
    parser.add_argument("--baseline", type=Path, nargs="+", default=[])
    parser.add_argument("--candidate", type=Path, nargs="+", required=True)
    parser.add_argument("--max-regression", type=float, default=.10, help="allowed relative frame loss (default .10)")
    parser.add_argument("--min-frame-ratio", type=float, default=.70, help="absolute stall floor (default .70)")
    parser.add_argument("--output", type=Path, help="write comparison JSON, including failures")
    args = parser.parse_args()
    if not 0 <= args.max_regression < 1 or not 0 < args.min_frame_ratio <= 1:
        parser.error("thresholds must be finite fractions: regression [0,1), floor (0,1]")
    try:
        result = compare([load(path) for path in args.baseline], [load(path) for path in args.candidate],
                         args.max_regression, args.min_frame_ratio)
    except (OSError, ValueError, KeyError, TypeError) as error:
        print(f"Invalid frame timing input: {error}", file=sys.stderr)
        return 1
    if args.output:
        args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(f"Cadence gate: {result['mode']}; allowed loss {args.max_regression:.0%}; stall floor {args.min_frame_ratio:.0%}")
    print("Window | Baseline frames/run | Candidate frames/run | B mean | A mean | A/B    | A median ms/run")
    for row in result["windows"]:
        baseline = ",".join(map(str, row["baseline_frames"])) or "-"
        candidate = ",".join(map(str, row["candidate_frames"]))
        mean = f"{row['baseline_mean_frames']:.1f}" if row["baseline_mean_frames"] else "-"
        ratio = f"{row['candidate_baseline_ratio']:.2%}" if row["candidate_baseline_ratio"] is not None else "-"
        medians = ",".join(f"{value:.4f}" for value in row["candidate_median_interval_ms"])
        print(f"{row['window']:>6} | {baseline:>19} | {candidate:>20} | {mean:>6} | {row['candidate_mean_frames']:>6.1f} | {ratio:>6} | {medians}")
    for failure in result["failures"]:
        print(f"FAIL: {failure}")
    print("FAIL" if result["failures"] else "PASS")
    return bool(result["failures"])


if __name__ == "__main__":
    sys.exit(main())

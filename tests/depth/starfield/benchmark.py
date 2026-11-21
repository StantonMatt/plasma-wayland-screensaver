#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Run with heavy, a capture-enabled build directory and a fresh evidence directory."""
import argparse
import json
from pathlib import Path
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--build", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    runner = Path(__file__).resolve().parents[2] / "run-depth-capture.py"
    args.output.mkdir(parents=True, exist_ok=False)
    results = []
    # The benchmark harness has its own high-speed defaults. Pin both the
    # actual Hyperspace defaults and the dense, long-exposure stress profile.
    profiles = {
        "default": ["AnimationSpeed=100", "AnimationDensity=50", "AnimationScale=100", "TrailAmount=35"],
        "dense": ["AnimationSpeed=300", "AnimationDensity=100", "AnimationScale=200", "TrailAmount=100"],
    }
    for profile, settings in profiles.items():
        directory = args.output / profile
        command = [sys.executable, str(runner), "--build", str(args.build), "--output", str(directory),
                   "--visual", "starfield", "--scene", "benchmark"]
        for setting in settings:
            command.extend(["--benchmark-set", setting])
        subprocess.run(command, check=True)
        for mode in ("independent", "seamless"):
            data = json.loads((directory / mode / "summary.json").read_text())
            cpu = {row["visual"]: row["labels"]["baseline"]["metrics"]["cpu_percent"]["mean"]
                   for row in data["visuals"]}
            for run in data["runs"]:
                if run["visual"] == "starfield":
                    log = (Path(run["directory"]) / "app.log").read_text()
                    if log.count("GPU-projected streaming star cells") < 3:
                        raise RuntimeError("Missing Starfield GPU evidence for the three outputs")
            star = next(row for row in data["visuals"] if row["visual"] == "starfield")["labels"]["baseline"]
            fps = {output: row["fps"]["mean"] for output, row in star["outputs"].items()}
            results.append(dict(fps=fps, profile=profile, mode=mode, total=cpu["starfield"], floor=cpu["none"],
                                net=cpu["starfield"] - cpu["none"], passed=cpu["starfield"] <= 15))
    report = json.dumps(results, indent=2)
    (args.output / "cpu.json").write_text(report + "\n")
    print(report)
    return 0 if all(row["passed"] for row in results) else 1


if __name__ == "__main__":
    sys.exit(main())

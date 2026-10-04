#!/usr/bin/python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Run under heavy; isolate the spawn-set experiment from production sources.

heavy python3 rust/snakes-core/examples/surge_scorecard.py --output ~/.cache/agent-scratch/plasma-wayland-screensaver/surge-scorecard
The lone enabled kind necessarily permits repeat spawns in this experiment.
"""
import argparse
import os
from pathlib import Path
from support.spawn_fixture import enabled_kinds, singleton_items
import shutil
import subprocess
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--output", type=Path)
parser.add_argument("--minutes", type=int, default=8)
parser.add_argument("--foundation", action="store_true", help="Use the staged foundation's three replaced source files")
parser.add_argument("--cpu", type=int, help="Pin the simulation process to this CPU")
args = parser.parse_args()
scratch = Path.home() / ".cache/agent-scratch/plasma-wayland-screensaver"
scratch.mkdir(parents=True, exist_ok=True)
root = args.output or Path(tempfile.mkdtemp(prefix="snakes-surge-scorecard-", dir=scratch))
root.mkdir(parents=True, exist_ok=True)
source = Path(__file__).resolve().parents[1]
repo = root / "repo"
crate = repo / "rust/snakes-core"
if crate.exists():
    raise SystemExit(f"Refusing to overwrite existing snapshot: {crate}")
shutil.copytree(source, crate, ignore=shutil.ignore_patterns("target", "__pycache__"))
# The Rust shader ABI checks include the repository's GLSL source by path.
shutil.copytree(source.parents[1] / "src/shaders", repo / "src/shaders")
if args.foundation:
    for relative in ["src/ai/mod.rs", "src/world/effects/mod.rs", "src/world/effects/surge.rs"]:
        staged = subprocess.check_output(["git", "show", f":rust/snakes-core/{relative}"], cwd=source)
        (crate / relative).write_bytes(staged)
effects = crate / "src/world/effects/mod.rs"
effects.write_text(enabled_kinds(effects.read_text(), "Surge"))
items = crate / "src/world/items.rs"
items.write_text(singleton_items(items.read_text()))
env = dict(os.environ, RUSTC="/usr/bin/rustc", RUSTDOC="/usr/bin/rustdoc", CARGO_NET_OFFLINE="true")
command = ["/usr/bin/cargo", "run", "--frozen", "--offline", "--release", "--manifest-path",
    str(crate / "Cargo.toml"), "--example", "surge_scorecard", "--", str(args.minutes)]
if args.cpu is not None:
    command = ["taskset", "-c", str(args.cpu)] + command
print("Isolated snapshot:", crate, flush=True)
print("Command:", " ".join(command), flush=True)
with (root / "matrix.txt").open("w") as log:
    result = subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT, timeout=600)
print((root / "matrix.txt").read_text(), end="")
raise SystemExit(result.returncode)

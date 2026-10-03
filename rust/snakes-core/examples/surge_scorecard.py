#!/usr/bin/python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Run under heavy; isolate the spawn-set experiment from production sources.

heavy python3 rust/snakes-core/examples/surge_scorecard.py --output /tmp/surge-scorecard
The lone enabled kind necessarily permits repeat spawns in this experiment.
"""
import argparse
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--output", type=Path)
parser.add_argument("--minutes", type=int, default=8)
parser.add_argument("--foundation", action="store_true", help="Use the staged foundation's three replaced source files")
args = parser.parse_args()
root = args.output or Path(tempfile.mkdtemp(prefix="snakes-surge-scorecard-"))
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
text = effects.read_text()
text, count = re.subn(r"pub const ENABLED_KINDS: &\[EffectKind\] = &\[[^;]+;",
    "pub const ENABLED_KINDS: &[EffectKind] = &[EffectKind::Surge];", text)
assert count == 1
effects.write_text(text)
items = crate / "src/world/items.rs"
text = items.read_text()
for old, new in [
    ("filter(|&&k|k!=self.last_item_kind)", "filter(|&&k|effects::ENABLED_KINDS.len()==1 || k!=self.last_item_kind)"),
    ("if k==self.last_item_kind {continue;}", "if effects::ENABLED_KINDS.len()>1 && k==self.last_item_kind {continue;}"),
]:
    assert text.count(old) == 1, old
    text = text.replace(old, new)
items.write_text(text)
env = dict(os.environ, RUSTC="/usr/bin/rustc", RUSTDOC="/usr/bin/rustdoc", CARGO_NET_OFFLINE="true")
command = ["/usr/bin/cargo", "run", "--frozen", "--offline", "--release", "--manifest-path",
    str(crate / "Cargo.toml"), "--example", "surge_scorecard", "--", str(args.minutes)]
print("Isolated snapshot:", crate, flush=True)
print("Command:", " ".join(command), flush=True)
with (root / "matrix.txt").open("w") as log:
    result = subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT, timeout=600)
print((root / "matrix.txt").read_text(), end="")
raise SystemExit(result.returncode)

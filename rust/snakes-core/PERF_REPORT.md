# Snakes performance work

Reference: v0.7.2 / `b684421`, same worktree and distribution toolchains.
No mechanics, ABI records/functions, quotas, horizons, or tactical policy changed.
No commit or review loop was run; review and the KWin harness belong to the orchestrator.

## Implementation

- `src/snakerenderer.cpp`: pack QColor channels once per primitive, use retained
  `std::vector` scratch buffers instead of Qt container detachment checks, and
  calculate ribbon normals once for outline and fill. Fixed head/crown colors
  use numeric constructors. Tessellation, LOD, interpolation, culling, draw order,
  and every emitted vertex/color remain identical in the reference fixtures.
- `rust/snakes-core/src/ai/mod.rs`: reuse candidate paths and rollout distances;
  don't copy paths into unused candidate slots when retaining a plan. Preserve
  full candidate evaluation for diagnostics. Avoid general remainder for bounded
  headings (bit-identical), stop recalculating saturated forecast envelopes, and
  walk only the existing nearby-rival bit mask in the existing slot order.
- `rust/snakes-core/src/ai/spatial.rs`: cache cardinal neighbors and boundary flags
  by arena topology; initialize tail-release values on their first occupancy-bit
  insertion instead of clearing two sparse tables every tick. Inline the hot
  coordinate-offset helper. All storage is allocated at controller construction;
  resize and reconfiguration reuse it.
- Tests cover poisoned candidate paths, stale sparse release values after resize
  and wall-mode changes, bounded-angle bit identity, and full diagnostics under
  plan reuse. Native geometry hashes were generated from the original renderer,
  not from the optimized implementation.
- `examples/ai_scorecard.rs` now reports tick p50. The native renderer test and
  `scripts/benchmark-snakes.sh` now include separate live step/export/first-window/
  second-window timings after six minutes of max-density/trails/IQ evolution.

## Profiling

`perf record -e cpu-clock -F 499 ...` was denied (`perf_event_paranoid=4`).
Fallbacks were AI's opt-in phase timers, Callgrind, and native QElapsedTimer
samples. Callgrind on the unmodified one-minute scorecard collected 7.417 billion
instructions: memset 18.65%, rollout's own instructions 15.58%, area search and
blocked-cell tests 9.66%, body queries 3.56%, and spatial offset 2.72%. These are
instruction shares, not CPU-time shares.

The original eight-minute, seed-20260814 / IQ100 / deadly phase profile spent
15.6% of AI time preparing shared data, 5.4% in strategy, 65.0% in rollout work,
and 13.5% in final area checks. Mechanics accounted for about 8% of total step
wall time. Opt-in timers have overhead; alternating unprofiled runs below are
used to assess performance.

The renderer emits 12 ribbon vertices per body edge (outline plus fill), about
3 marking vertices per body segment, plus heads, tails, eyes, crowns and food.
Neither the segment count nor vertex count was reduced. Export/copy is roughly
1–1.5 microseconds in mature states and is not the limiting cost.

Simulation and export already run once on the shared timeline, regardless of
window count. Both windows independently tessellate: their measured costs are
almost equal. QSG nodes cannot be shared across graphics contexts; CPU geometry
could be cached only for matching frame, interpolation/pulse time, viewport,
scale, offset, palette and settings. Existing mixed-refresh and delayed-window
leases require independent samples. No cross-window cache or synchronization was
added; per-window work is now much cheaper.

## Measurements

Release builds, i9-13900K, Qt 6.10.2, GCC 15.2, rustc 1.93.1. Paired runs alternate
order and use the same pinned CPU for each workload. CPU frequency/scheduling
varies, so compare paired medians rather than isolated best samples.

Initial renderer optimization (same triangles): 0.467 -> 0.138 ms/frame for packed
colors and vertex storage in the first short sample. A later three-pair sample
with all container buffers converted measured 0.5643 -> 0.199 ms/frame. Final
paired measurements with shared ribbon normals follow below.

Incremental Rust experiment medians on CPU 4, eight minutes, seed 20260814,
IQ100/deadly, mean tick time:

| Implementation | ms/tick |
|---|---:|
| Reference | 0.2787 |
| Cached grid topology | 0.2598 |
| Plus bounded-angle fast path | 0.2589 |
| Plus candidate/distance reuse | 0.2518 |

Sparse release initialization and saturated forecast envelopes are included in
final measurements. Nearby-rival bit walking was a small/noisy change. Forced
inlining of blocked-cell helpers gave no improvement and was discarded. Inlining
the offset helper reduced the separate IQ100/wrap pilot median 0.2454 -> 0.2426 ms.

The 6000-segment / 450-food cap test (CPU 10, three alternating pairs) measured
median mean time **0.5001 -> 0.4377 ms**, p95 **0.5782 -> 0.4965 ms**. Both original
and optimized allocation tests report zero allocations through cap stepping,
reconfiguration, deaths and respawns. The cap is an adversarial upper bound,
not the typical natural ecosystem.

Final three alternating pairs on CPU 8 (median of each run's p50, milliseconds):

| Work | Original | Optimized | Change |
|---|---:|---:|---:|
| Synthetic mature geometry, mean/frame | 0.4136 | 0.1170 | -71.7% |
| Six-minute deadly ecosystem step | 0.2958 | 0.2413 | -18.4% |
| Six-minute wrapping ecosystem step | 0.3228 | 0.2741 | -15.1% |
| Deadly export/caller-buffer resize | 0.001088 | 0.001104 | noise |
| Wrapping export/caller-buffer resize | 0.001269 | 0.001261 | noise |
| Deadly first-window sync/build/copy | 0.2898 | 0.0836 | -71.1% |
| Deadly second-window sync/build/copy | 0.2874 | 0.0824 | -71.3% |
| Wrapping first-window sync/build/copy | 0.4189 | 0.1667 | -60.2% |
| Wrapping second-window sync/build/copy | 0.4147 | 0.1644 | -60.4% |

These ecosystems have exactly the same population and vertices before/after:
deadly mean 1082.2 segments / 206.98 food / 31737.8 vertices; wrapping mean
1309.18 segments / 212.87 food / 35959.4 vertices. The first and second windows
use identical scale, palette and interpolation, and each is explicitly built.

The final standalone cap allocation run measured mean 0.4132 ms, p95 0.4482 ms,
p99 0.4842 ms, with all 6000 segments retained and zero allocations. This isolated
sample is supplementary; use the paired cap medians above to compare versions.

**Harness expectation:** against 0.7.2 in identical states, expect GUI simulation
work around **0.82–0.85x** and renderer sync around **0.29–0.40x**. Applied to the
brief's 0.45/0.37 ms medians, that predicts approximately **0.37–0.38 ms GUI** and
**0.11–0.15 ms renderer sync** on that harness, not a proven 0.26 ms GUI result.
Native deadly simulation meets 0.26 ms, wrapping misses by roughly 0.014 ms;
rendering meets 0.22 ms in both modes. Process CPU is not measured here: frame
pacing, render-thread/RHI work and GPU/driver costs can change the total. Use the
same seed, settings, output geometry, FPS, startup phase and sampling interval;
compare segment/food counts as well as CPU so longer survival isn't mistaken for
slower code. Repeat the one/two-output 30/60 FPS matrix. No vertices were removed,
so GPU tessellation/upload load follows exactly the existing 0.7.2 ecosystem.
Returning overall CPU to 0.7.1 levels remains a KWin harness validation gate.
If it misses despite the renderer improvement, the next measurable lever is
body-ribbon decimation or shared CPU geometry for matching presentation samples;
those would need visual or timing/lifetime validation beyond this exact-geometry
optimization.

## Reproduction and validation

All Cargo commands use the distribution compiler, frozen lockfile and offline
mode. Build outputs and the isolated original source/library live under ignored
`build-perf/`; raw logs and immutable experiment binaries are `/tmp/snakes-perf-*`.

```sh
cmake -S . -B build-perf -G Ninja -DCMAKE_BUILD_TYPE=Release -DBUILD_TESTING=ON
cmake --build build-perf -j 6
ctest --test-dir build-perf --output-on-failure
RUSTC=/usr/bin/rustc /usr/bin/cargo test --release --frozen --offline \
  --manifest-path rust/snakes-core/Cargo.toml
RUSTC=/usr/bin/rustc /usr/bin/cargo test --release --frozen --offline \
  --manifest-path rust/snakes-core/Cargo.toml --test allocation \
  zero_allocations_with_ai_at_caps_and_after_reconfiguration -- --nocapture
RUSTC=/usr/bin/rustc /usr/bin/cargo build --release --frozen --offline \
  --manifest-path rust/snakes-core/Cargo.toml --example ai_scorecard
taskset -c 4 rust/snakes-core/target/release/examples/ai_scorecard 8 --ai-only
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software taskset -c 8 \
  build-perf/bin/test-snakerenderer geometryFingerprint benchmarkMatureGeometry \
  benchmarkEcosystemPhases -iterations 3000 -o -,txt
bash -n scripts/benchmark-snakes.sh
git diff --check
```

Geometry hashes and vertex counts are identical to the reference in all three
fixtures (deadly, wrapping, developer, including malformed segments). Exact
12-world scorecard totals remain **156 kills / 49 self deaths / 0 wall deaths**;
all non-timing scorecard fields also match exactly. No behavior tuning was used.

Final validation: full CTest **14/14 passed**, including native/QML, ABI, mechanics
parity and recorded parity (`/tmp/snakes-perf-ctest-final.txt`). The last build
has no warnings. The explicit Rust cap allocation command passed, the complete
12-world scorecard remained identical, `git diff --check` and shell syntax passed,
and the complete updated benchmark wrapper succeeded with:

```sh
SNAKE_BENCHMARK_BUILD_DIR="$PWD/build-perf" \
SNAKE_BENCHMARK_RESULTS_DIR=/tmp/snakes-perf-script \
  taskset -c 12 scripts/benchmark-snakes.sh
```

The wrapper produced `/tmp/snakes-perf-script/snakes-native-20261002T142521Z.log`
and `/tmp/snakes-perf-script/snakes-renderer-20261002T142521Z.csv`. Final paired
pipeline log: `/tmp/snakes-perf-paired-pipeline-final.txt`; original and final
scorecards: `/tmp/snakes-perf-before-matrix.txt` and `/tmp/snakes-perf-last-matrix.txt`.
Final opt-in AI profile: `/tmp/snakes-perf-last-profile.txt`. All commands are
one-shot; no server, watcher, or background benchmark remains running.


## S2 (0.12.0) prism release slice

See [R12_PRISM_REPORT.md](R12_PRISM_REPORT.md) for the ABI-v3 prism scheduler,
AI, renderer, focused checks and final paired measurements against 0.11.0.
Mature geometry is 1.003x baseline; chaos/matched mature is 1.047x at 8736
vertices. Standard mean tick delta is +0.01215 ms and reference delta is
-0.00205 ms; allocation checks stay at zero. Desktop capture, process CPU and
frame-pacing gates remain unverified.

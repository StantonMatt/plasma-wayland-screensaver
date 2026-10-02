# Rust/JS mechanics parity results

Verified on 2026-10-02 with `/usr/bin/cargo` and `/usr/bin/rustc` 1.93.1,
Python 3.14.4, Qt 6.10.2, Release CMake/Ninja builds. The recorded JS fixtures,
not `rust/snakes-core/src/world/golden.rs`, are the authority.

All **15 fixtures pass**: **3,904 ticks**, **1,759 sampled frames**, and
**1,862 ordered events**. Discrete outcomes, event ticks, death reasons/victims/
killers, growth counts, respawns, food IDs/ownership/counts and sampled RNG
states/draw counts match exactly. Every continuous observation is within
**1e-6**, including every recorded sample through tick 900. Maximum absolute
error is **4.9999835e-7**, consistent with six-decimal fixture rounding.
No early or late divergence occurred; no chaotic-roundoff exception was used.

| Fixture | Ticks | Samples | Events | Maximum absolute error | Result |
| --- | ---: | ---: | ---: | ---: | --- |
| body-hit | 8 | 9 | 3 | 4.99206521e-7 | PASS |
| deadly-walls | 900 | 361 | 406 | 4.99997725e-7 | PASS |
| death-burst-food | 8 | 9 | 2 | 4.99618181e-7 | PASS |
| default-wrap | 900 | 361 | 506 | 4.99998322e-7 | PASS |
| feeding-growth-insertion | 24 | 25 | 4 | 4.98505e-7 | PASS |
| head-on-equal | 4 | 5 | 4 | 4.99997327e-7 | PASS |
| head-on-length-difference | 4 | 5 | 7 | 4.9173201e-7 | PASS |
| maximum-density-trails | 900 | 361 | 406 | 4.9999835e-7 | PASS |
| respawn | 190 | 191 | 5 | 4.99967996e-7 | PASS |
| self-collisions | 900 | 361 | 506 | 4.99998322e-7 | PASS |
| self-exclusion | 2 | 3 | 3 | 4.97248038e-7 | PASS |
| vacuum-lock | 40 | 41 | 3 | 4.99938324e-7 | PASS |
| world-resize | 12 | 13 | 2 | 4.98505e-7 | PASS |
| wrap-seam-collision | 4 | 5 | 4 | 4.94040137e-7 | PASS |
| wrap-seam-crossing | 8 | 9 | 1 | 4.98486045e-7 | PASS |

Through tick 300, maxima equal the full-run maxima except deadly-walls, whose
early maximum is 4.99993007e-7. Its full-run maximum occurs at tick 340.
The global maximum occurs at tick 127, snake 12, segment 5, x coordinate.

The replay imports full-precision initial mechanics state once and runs all
intermediate ticks. Expected output does not drive the simulation: only the
recorded input actions, sample schedule, and independent steering script enter
the runner. Death attribution is captured before any snake explodes. Events
from unsampled ticks remain ordered and retain their actual tick numbers.
The private dt step supports the focused vacuum trace; public stepping remains
30 Hz. Snapshot/event hooks exist only with the opt-in `parity` feature.

The numerical mechanics required no corrections. One default configuration
correction was made: `Config::default().palette_size` is now **6**, matching
all built-in `VisualUtils.colors` palettes, instead of 7. The initial population
check verifies snake colors wrap after slot 5 and all ambient color indices
are valid. Replay explicitly supplies six palette entries. No existing C ABI
functions, `repr(C)` layouts, or controller code changed. There are **no
intentional behavior differences** in the tested mechanics.

Commands run from the worktree root:

```sh
/usr/bin/cargo build --frozen --offline --manifest-path rust/snakes-core/Cargo.toml \
  --release --features parity --example parity
/usr/bin/cargo test --frozen --offline --manifest-path rust/snakes-core/Cargo.toml
/usr/bin/cargo test --frozen --offline --manifest-path rust/snakes-core/Cargo.toml \
  --features parity
cmake -S . -B build-parity -G Ninja -DCMAKE_BUILD_TYPE=Release -DBUILD_TESTING=ON
cmake --build build-parity -j 4
ctest --test-dir build-parity --output-on-failure
python3 tests/parity/snakes/compare.py \
  --binary build-parity/cargo-target/Release/release/examples/parity
python3 -B tests/parity/snakes/test_compare.py
/usr/bin/cargo run --frozen --offline --manifest-path rust/snakes-core/Cargo.toml \
  --release --example bench_mechanics
git diff --check
```

CMake configure/build passed. Full CTest passed all **10 tests**, including
Qt integration, C ABI, feature-free Rust tests and `snakes-parity`. Both Rust
feature configurations passed **48 test invocations** each, including the cap
allocation test (zero steady-state allocations, including death and respawn).
Six Python negative-policy checks passed: rounding, exact food IDs/owners,
RNG/lengths, unsampled event metadata, late numerical divergence, and nonfinite
output. The production static archive contains no parity hook symbols.
Whitespace checks passed.

Final ordinary benchmark: **0.014945 ms/tick** average for 14,400 ticks,
p50 13.992 us, p95 25.667 us, p99 29.357 us, max 118.841 us, versus the supplied
0.0164 ms/tick baseline. Unpinned runs varied (one earlier run was 0.022199),
so the original staged crate was also reconstructed under the ignored
`build-parity/benchmark-baseline` directory and built offline:

```sh
RUSTC=/usr/bin/rustc CARGO_TARGET_DIR="$PWD/build-parity/benchmark-baseline-target" \
  /usr/bin/cargo build --frozen --offline \
  --manifest-path build-parity/benchmark-baseline/Cargo.toml \
  --release --example bench_mechanics
taskset -c 0 build-parity/benchmark-baseline-target/release/examples/bench_mechanics
taskset -c 0 rust/snakes-core/target/release/examples/bench_mechanics
```

After one warmup of each binary, six paired runs alternated order on CPU 0.
Each run simulated 14,400 ticks. Population/food/death summaries matched in
every pair. Raw averages in ms/tick:

| Pair | Original | Final |
| --- | ---: | ---: |
| 1 | 0.014380 | 0.014117 |
| 2 | 0.014171 | 0.014149 |
| 3 | 0.014194 | 0.014366 |
| 4 | 0.014016 | 0.013981 |
| 5 | 0.014106 | 0.014015 |
| 6 | 0.014056 | 0.014109 |
| Median | 0.014139 | 0.014113 |
| Mean | 0.014154 | 0.014123 |

No measured slowdown: paired median improved 0.18%, effectively unchanged
within measurement noise. Raw data is in the ignored build artifact
`build-parity/benchmark-comparison-cpu0.json`.

No unfinished implementation or decisions. AI, rendering, accumulator behavior,
unrecorded settings and unobserved fields remain outside this fixture proof.
No commits, pushes, review loops, servers or watchers were started. All one-shot
processes exited; build artifacts remain only in ignored build/target locations.

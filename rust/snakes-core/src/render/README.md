# R1 step C: Rust classic renderer

The C++ renderer now owns one `QSGGeometryNode`/vertex-colour material and a
per-node Rust render handle. Rust builds triangles directly into the caller's
retained QSG vertex buffer. Only buffer growth requires a count/retry call;
steady-state builds allocate nothing and make no intermediate vertex copy.
Interpolation, ribbons, discs, markings, heads/eyes/crowns, food, vacuum streaks,
wrap copies, clipping, viewport transforms, LOD hysteresis and steering overlays
are implemented here. No simulation mechanics or AI sources changed in step C.

The R1 additions are a 35% white boost tint, a 15-entry tail ring (at most 14
edges per boosting snake, plus seam copies), small dim four-sided pellets, two
12-sided kill-flash discs lasting 0.5 simulation seconds, hunting/trapped eyes,
corpse fading over 0.55 seconds, and crowns controlled exclusively by LEADER.
Contrail samples have tick timestamps, so skipped window presentations cannot
stretch their lifetime beyond 0.5 seconds. Simulation replacement, tick rewind,
and geometry-generation changes clear history. Repeated frame presentations and
buffer retries cannot duplicate trail samples or kill events.

## Additive render ABI

The simulation ABI stays v2; its existing functions and records are unchanged.
The header adds:

- Opaque `snakes_core_renderer`, with `snakes_core_render_create`,
  `snakes_core_render_destroy`, `snakes_core_render_reset` and
  `snakes_core_render_build`.
- `snakes_core_render_color`: 4 RGBA bytes, alignment 1.
- `snakes_core_render_vertex`: x/y f32 plus RGBA, 12 bytes, alignment 4;
  RGBA offset 8. C++ asserts compatibility with `QSGGeometry::ColoredPoint2D`.
- `snakes_core_render_params`: viewport dimensions, scale/offset, interpolation,
  presentation time (eight f64 values), wall/developer flags (two u32 values);
  72 bytes, alignment 8.
- `snakes_core_render_output`: required vertex count (`size_t`), dense-food flag
  and reserved zero (`u32` each); 16 bytes/alignment 8 on x86_64.

Buffers are borrowed only for the call. Handles require serialized access.
`BUFFER_TOO_SMALL` writes a prefix and the complete required count; grow and
retry the same frame. Invalid arguments leave output and history untouched.
Scratch is allocated at construction for the existing ABI limits (14 snakes,
1600 segments per snake). Caller vertex storage grows to its high-water count.

## Equivalence and verification

Before removing the C++ generator, a temporary migration gate built six fixtures
independently in C++ and Rust and compared every vertex byte. All six passed.
The final C++ tests retain their original-C++ SHA-256 constants and optional
raw-buffer comparisons using `SNAKES_RENDER_REFERENCE_DIR`. Rust tests also
retain three original-C++ FNV-1a constants. Reference files are under
`/tmp/snakes-render-reference`; the pre-port benchmark executable is
`build-render/bin/test-snakerenderer-cpp-baseline` (both outside tracked source).

| Fixture | Vertices |
|---|---:|
| Walls, malformed segment, dense food | 43794 |
| Wrapping, malformed segment, dense food | 23358 |
| Developer overlay, malformed segment | 44046 |
| Sparse food, rotated heads, palette alpha | 3249 |
| Viewport scale and offset, wrapping overlay | 1716 |
| Wrapped vacuum streaks and arena edges | 2448 |

Exact principal commands (from the worktree):

```sh
cmake -S . -B build-render -G Ninja -DCMAKE_BUILD_TYPE=Release -DBUILD_TESTING=ON
/home/mjstanton/.local/bin/heavy cmake --build build-render -j 4
/home/mjstanton/.local/bin/heavy bash /tmp/snakes-render-baseline.sh
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  SNAKES_RENDER_REFERENCE_DIR=/tmp/snakes-render-reference \
  build-render/bin/test-snakerenderer geometryFingerprint \
  r1FlagsPelletsAndKillEventsReachRustGeometry crownFollowsLeaderFlagOnShorterSnake \
  replacingSimulationClearsRustHistoryAtSameTick -o -,txt
/home/mjstanton/.local/bin/heavy bash /tmp/snakes-render-final-checks.sh
git diff --check
```

The baseline script rebuilt the original C++ renderer test, captured six raw
reference buffers, copied its executable, and ran three geometry/sync samples.
The final-check script ran `ctest --test-dir build-render --output-on-failure`,
then three alternating pairs (C++/Rust, Rust/C++, C++/Rust), using these commands
for each executable:

```sh
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software taskset -c 8 "$binary" \
  benchmarkMatureGeometry -iterations 10000 -o "$geometry_log",txt
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software taskset -c 8 "$binary" \
  benchmarkMatureSyncFrame -iterations 1000000 -o "$sync_log",txt
```

Full Release build passed. Full CTest **14/14 passed**, 75.66 seconds, including
**327 Rust test executions** (10 new render tests), QML/simulation/overlay tests,
ABI and unchanged Classic parity/fixture recordings. The new allocation test
measures 1000 render ticks, including boosts, pellets, kill flashes, corpses and
history resets, and reports **zero allocations**. Focused final native tests
passed 11/11 (six reference fixtures, three R1 integration cases, init/cleanup).
`git diff --check` passed.

## Performance gate

Same machine, Release, Qt 6.10.2, GCC 15.2, distribution rustc 1.93.1; CPU 8.
Pre-port C++ uses 0.7.3 geometry with the staged Step A snapshot integration.
Synthetic snapshots explicitly retain the pre-R1 equal-length crowns in both
implementations. All timing values below are ms/call.

| Pair | C++ geometry | Rust geometry | C++ sync | Rust sync |
|---|---:|---:|---:|---:|
| 1 | 0.1494 | 0.0422 | 0.000012 | 0.000004 |
| 2 | 0.1168 | 0.0425 | 0.000012 | 0.000004 |
| 3 | 0.1209 | 0.0442 | 0.000012 | 0.000004 |
| Median | **0.1209** | **0.0425** | **0.000012** | **0.000004** |

Geometry is **0.352x** the measured baseline; sync is **0.333x** (Qt's whole-ms
reporting quantizes the very small sync measurements). Both meet the <=1.0x
step-C gate. Geometry is also below PERF_REPORT.md's historical optimized
0.1170 ms/frame. Raw pair logs: `/tmp/snakes-render-pair-*.txt`.

The full-suite live ecosystem checks reported first/second-window means of
0.03022/0.02965 ms for walls and 0.05951/0.05775 ms for wrapping. These are
additional observations, not paired release comparisons: the V2 ecosystems
have different populations from the historical Classic performance report.

## Limits and handoff

Corpse fade starts at the first corpse snapshot observed by that window; ABI v2
exports no corpse-age counter. Simulation retention still bounds its final
removal. Qt 6.8/6.9 retain compatibility with an extra rebuild when the vertex
count changes; the direct retained-buffer performance gate was measured on
Qt 6.10.2. No real KWin/GPU visual harness was run by this worker.

Changed files: this `render/` module, `src/lib.rs` (module declaration),
`src/ffi.rs` and `include/snakes_core.h` (additive ABI), `src/snakerenderer.*`,
`tests/test_snakerenderer.cpp`, and `tests/render.rs` (crate paths relative to
`rust/snakes-core`). Step A's staged changes remain intact. No commit, push,
review loop, server or watcher was started; all task commands have exited.

## Review fixes: stationary snapshots and complete seam bounds

Corpses and FROZEN snakes render their current segment positions. The same
position policy drives bodies, attached eyes/crowns, and steering overlays;
trail sampling stops and clears its ring when a snake dies or freezes, even if
a stale BOOSTING flag remains. Respawn/generation, rewind, and geometry changes
retain their existing history resets. Food/pellets already use current records
without interpolation; removed records cannot replay previous positions.

All arena copy selection now uses the shared bounds selector in geometry.rs.
Flash bounds grow with the outer radius; contrail edges emit both seam sides
(and corner copies), instead of stopping at the first visible copy. Food bounds
include halos, offset highlights, and the complete vacuum streak and width.
Body copy bounds and clipping include minimum pixel sizes of eyes and crown
strokes/gems under small or anisotropic scaling. Steering arrows include their
wing lengths and widths. Zero-scale projections emit no geometry.

Regression coverage includes stale interpolation across physics boundaries,
edge/corner flash expansion, pellet/food copies, vacuum streaks extending beyond
halos with anisotropic scaling, contrails across both seams and death, minimum
pixel sizes, and zero-scale projections. The allocation test also exercises
wrapping flashes, pellets/vacuum streaks, steering overlays, corpses, and resets.
All three Rust and five of six C++ original geometry fingerprints remain
unchanged. The C++ scaled/offset wrapping fixture changes from 1716 to 1761
vertices: complete halo bounds add a 63-vertex food copy, while consistent arena
bounds remove an 18-vertex steering copy outside the arena. Its updated hash
covers the corrected scene; optional original-C++ buffer comparison applies to
the five unchanged fixtures.


Final fix verification: full Release build passed without compiler warnings;
full CTest **14/14 passed** in 38.45 seconds (including Classic parity), and
frozen/offline Release Cargo tests passed **354 test executions**. The expanded
1000-tick render allocation check reports **zero allocations**. Three paired
production-library measurements on CPU 8 retained identical mature-fixture
vertex counts: walls **0.046616 -> 0.045913 ms** (0.985x), wrapping
**0.071024 -> 0.061939 ms** (0.872x). Walls are roughly unchanged within sample
variation; wrapping is 12.8% faster. Canonical coordinates bypass the remainder
call, and extra pixel-minimum bounds work runs only below a 2.4-pixel body radius.
Exact commands, sibling audit, and logs are in build-fix1/WORKER_REPORT.md,
build-fix1/run-final-checks.sh, and build-fix1/timing-optimized/ (gitignored).

## Numeric boundary and wrap budget

Both the render ABI and direct Rust builds use the same frame/projection and
snake numeric guards. Coordinates, dimensions, radii, angles, phases and times
are bounded to a magnitude of `1e9`; scale is zero (empty projection) or within
`1e-6..=1e6`, and interpolation/food attraction is within `0..=1`. These limits
leave normal world exports unchanged and keep derived geometry representable in
f32. Invalid frames/snake ABI arguments are rejected before touching history;
invalid food/events are skipped and invalid segments retain the existing
malformed-point behavior. The guards also cover exported f64-to-f32 overflows.
History uses the same guards for flashes and contrails.

All primitive bounds share one copy-range helper. It validates finite bounds
before deriving translation ranges, then emits the complete range (including
translations beyond the eight neighboring arenas). Derived ranges that exceed
4096 total copies, overflow integer bounds, or are nonfinite are rejected as a
whole; they are never truncated to a partial tiling. This explicit work budget
keeps extreme finite input/tiny arenas bounded as well as nonfinite records.

Sequentially unwrapped bodies spanning more than one arena use per-edge bounds.
Each shortest wrapped edge spans at most half an arena on each axis, so its copy
count is bounded by `(ceil(2 * margin_x / arena_width) + 2) *
(ceil(2 * margin_y / arena_height) + 2)`, capped by the shared primitive budget.
A body of `n` segments therefore visits at most `3 * (n - 1) * 4096` copies,
independent of how many arenas its complete unwrapped bounding box covers.
Only the edge and its immediate neighbors are mapped to retain smooth normals;
outline, fill, and decoration passes preserve layering across sections. Tail
caps, markings, heads, eyes, and crowns emit once per needed copy. Ordinary
bodies retain the original mapping path and geometry fingerprints. Both paths
use retained scratch and allocate nothing during builds.

Sibling coverage: contrail edges (including samples outside the original
arena), expanding flashes, oversized food halos/highlights and vacuum streaks,
body/head/eye/crown pixel minimums, and steering arrows all use the complete,
bounded copy selector. Tests cover horizontal/vertical/diagonal multi-arena
bodies, reverse wrapping, maximum body length, small arenas, anisotropic
synchronized viewport transforms, nonfinite food, and extreme finite geometry.
Magnet/nova radius visuals are not currently tessellated here; future such
primitives must use the same complete bounds and work-budget policy.


Multi-arena fix verification: frozen/offline Release Cargo tests passed **370
test executions**; Release build and full CTest in `build-fix1` passed **14/14**
(45.21 seconds). Both the 1000-tick all-effects allocation regression and the
1600-segment multi-arena body regression report **zero allocations**. Three
paired production-library benchmark samples on CPU 8 retain identical vertex
counts; median walls geometry is **0.058079 -> 0.058195 ms** (+0.20%) and
wrapping is **0.074702 -> 0.073477 ms** (-1.64%), within the 5% gate. Exact
commands, regression evidence, sibling audit and logs are in
`build-fix1/WRAP_FIX_REPORT.md`, `wrap-checks.sh` and `wrap-*.log` (gitignored).

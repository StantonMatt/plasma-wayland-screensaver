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

## R2 shader renderer

`render/shader.rs` adds `snakes_core_render_build_shader`, sharing borrowed
snapshots and history with classic but emitting a 24-byte vertex directly into
QSG storage: position f32x2, across/along f32x2, RGBA u8x4, params u8x4.
Body along coordinates count from the tail. The four params bytes mean:

| Primitive | kind | byte 1 | byte 2 | byte 3 |
|---|---:|---|---|---|
| Body | 0 | tier bits 0–1, active effect bits 2–4, neck-marking suppression bit 5, white crown palette bit 6, reserved bit 7 | simulation flags except bits 1/4/5 encode wave origin (1..5 item, 6 white, 7 crown, 0 none) | interpolated wave brightness, or corpse fade |
| Head | 1 | tier bits 0–1, deterministic phase bits 2–5, white crown palette bit 6, kill flare bit 7 | simulation flags | desired-heading offset |
| Spark / shard / pellet | 2 / 3 / 4 | phase | remaining life fraction | reserved |
| Vacuum / debug direction | 5 | reserved | reserved | reserved |
| Continuous contrail | 10 | reserved | reserved | reserved |
| Impact / succession / boost ring | 6 / 7 / 9 | event seed | reserved | normalized effect age |
| Prism / PrismSeed (S2) | 8 | phase | ripe life / seed progress | 255 seed; 0..254 ripe age over 1 s (0.6 s in Calm) |
| Acid glow (S3, colour alpha = strength) | 25 | 0 | 0 | 0 |

Kinds 11–31 were reserved for item and power-up sprites in R2; subsequent
allocations are documented below. No atlas is needed until R3.
All allocations occur when the render handle or retained QSG buffer is created
or grown. Taper multipliers and shader alpha bytes are cached per snake length, using the exact shared
`shape::taper` profile. Normals are prepared once per snake, with bounded f64
square roots. Live wall-bounded ribbons convert each shared point pair once; corpse and
wrapped primitives convert four corners per edge to preserve drift and exact
f64 seam translations. Both write six triangle-list vertices with a single
capacity check. A conservative dot/cross lower bound rejects ordinary bends
before the exact circumradius calculation, and clipping limits are prepared
once per point. Wave updates touch only the
seven-segment bands, with at most two waves per snake. Effects retain eight
quads; contrails retain at most fourteen edges (84 vertices) before seam copies.
Body, head and food each retain their six-vertex primitive budget.

`SnakeMaterial` owns one baked shader pair. Tube bands, additive glow, contact
shadow, sheen, tier markings/spine lights, eyes, tongue, crown, bow waves and
food halo twinkle run in the fragment shader. Additive pixels carry zero alpha
under Qt's premultiplied-over blend state. All geometry generation stays in
Rust. The software backend and unavailable shader resources use the unchanged
classic generator and its original geometry fingerprints.

The host can call `SnakeRenderer::setShaderTimeFrozen(bool)` (QML property
`shaderTimeFrozen`). The host freezes both material clocks and the geometry's
presentation time at the captured instant, including discrete blinks/tongue
flicks, transient rings, waves, contrails, flashes and corpse dissolution.
It forwards `snakes_core_render_set_reduced_motion` for both geometry formats
from the same place. The Rust flag selects reduced-motion styling/durations;
callers must also hold presentation time fixed to pause animation. Simulation
snapshots and interpolation remain independent of this render-time clock.
No overlay, clock or setting code is changed by the render implementation.

The material uses `qt_add_shaders(BATCHABLE ...)` and the `qt6-shadertools-dev`
build dependency. The pack contains standard and batchable vertex variants for
SPIR-V 100, HLSL 50, MSL 12, GLSL ES 100/300/310/320, and desktop GLSL
120/130/140/150/330, plus the matching standard fragment variants. Renderer
integration tests inspect that exact resource pack and its `_qt_order` input.
There is no shader-tools runtime dependency.

Runtime selection uses `QShader::fromSerialized` and `availableShaders()` to
check both vertex variants and the fragment variant against the window's actual
QRhi backend and OpenGL context version/profile. Before creating shader geometry,
a one-time pipeline probe compiles both variants on that same QRhi, using the
window's render pass, sample count, vertex layout, uniform binding and blend
state. Missing variants, unavailable RHI/render target, native pipeline creation
failure, and reported scene-graph errors select classic rendering. The node
retains this decision; decoding, capability checks and driver probes do not run
per presentation. The probe submits no commands and reads no pixels.

QShader/QRhi use Qt's RHI API, which has limited compatibility guarantees. On
Debian/Ubuntu its headers require `qt6-base-private-dev` matching `QtGui`, in
addition to the existing build dependencies. CMake reports the missing package
explicitly. No Qt private scene-graph implementation API is used.

### Native visual capture

On the real desktop, capture the five Anatomy rows (hatchling/adult/elder/
titan/leader), all four food sprites, a visible boost contrail and a tight coil:

```sh
QT_QUICK_BACKEND=rhi QSG_RHI_BACKEND=opengl \
  SNAKES_CAPTURE_PATH=/tmp/snakes-r2.png \
  build-r2/bin/test-snakerenderer captureShaderFixture
```

The test is skipped during ordinary software-backend CTest runs. Its `shader`
row requires a working RHI and verifies that shader rendering is active, as well
as checking the output image. On OpenGL the `driver-rejection-fallback` row
deliberately supplies invalid GLSL to the pipeline probe, verifies classic
rendering is selected, and saves a second visible image to
`$SNAKES_CAPTURE_PATH.fallback.png`. That row skips on Vulkan; the shader row
can also be run with `QSG_RHI_BACKEND=vulkan`. This sandbox has
no `/dev/dri`: offscreen OpenGL cannot create a context, offscreen Vulkan is
unsupported, and Xvfb cannot connect through the sandbox. GPU appearance and
frame pacing must therefore be checked on the real display. The local HTML
prototype was unavailable through the browser's local-file URL policy; the
implementation used its drawing source as the visual reference.

Known visual approximations: the head uses an analytic spade distance field
rather than the prototype's Bézier path; its quad extends to 2.9 head units
forward to contain the full tongue while retaining six vertices. The final
body edge ends at a pointed tip within the shared taper envelope. Shard expiry
uses a nominal 30-second lifetime because ABI v2 exports a life fraction but
not the randomized original lifetime (18–34 seconds). A boost ring replayed
from compact tail-only history is anchored at the first full head sample,
while retaining its original start time. Performance/verification results and
exact paired commands are recorded in `build-r2/WORKER_REPORT.md` and
`build-r2/LOOK_FIX_REPORT.md` (gitignored).


### Product-review corrections

Body colour alpha carries the local taper, while across UV carries physical
extrusion in radii / BOUNDS_BODY (2.5), including a curvature clamp on tight bends.
Interpolating physical distance fixes tapered trapezoids' diagonal kink; the
last edge's silhouette distance tapers to a point without dividing its halo by
zero. Corpse opacity lives in params byte 3, independently of width. Corpses
flash white, then keep a 35% white tint. Shared across/silhouette derivatives
anti-alias the tube bands, shadow and halo. Chevrons occur every two segments,
with a 0.20r stroke, and saddle bands are 0.50r wide every six segments.

The head smoothly joins the neck (smooth-max 0.15 head units), continues tube
bands and fades its rear shadow into the underlying body. Its colour alpha is
the segment-1 neck taper. Boosting widens the quad to contain both bow strokes
without stretching head units. Stored per-tick headings interpolate on the
shortest arc with the same factor as the body; frozen heads use current angles.
Ember and mono are recognized by their complete shipped palettes once per
build and pass an explicit white-crown bit; individual orange/yellow colours
never decide the crown's colour.

Opaque coverage masks only the outer halo. A separate additive term retains
light waves, spine dots and crown glow on skin, while eye light is applied
before the socket and iris. Inner halo strength is 1.55x; the outer edge fades
across 0.25 widths and the head has a matching front cap. Spine dots use radii
on both axes. A circumradius limit of 0.95 times local curvature radius bounds
the glow envelope; ordinary paths avoid its divide/sqrt. It adds no vertices,
allocations, or draw calls.

The shader normal walk carries raw finite-point checks across its three-point
window and uses one reciprocal for both normal components. Live wall-bounded
ribbons compute the glow envelope once per snake and convert shared corner
pairs once, only for visible edges. Edges starting inside the viewport avoid
the full bounds calculation; boundary edges retain it. Corpse and wrapped
paths preserve their separate drift and exact seam translations.
Per-length taper bytes are retained, with no steady-state allocation.

Contrails have kind 10, constant along UV, and shared per-end width/colour/alpha
(half-width 0.465r*u, alpha 0.35*u², 35% white). Ordinary food keeps its exported size;
the prism family uses a 1.6x display factor without changing simulation size.
Only halo brightness twinkles. Shards use a slim rhombus, and prism fruit has
an iridescent pearl, circular spectral halo and eight dispersion rays. Vacuum streaks
use half-width 0.65s, length s*(3+8*attraction), 0.75 peak and 40% white tint.

The deterministic 1280x720 capture fixture uses black, the shipped ocean
palette, R0=0.0108*720, arc-length spacing 1.18r on the Anatomy sine paths,
lengths 16/48/110/260/280, and tangent-aligned heads. Two extra snakes expose
a full boost tail and a tight spiral. The food row uses actual representative
exported sizes (spark 0.321R, shard 0.3405R, pellet 0.295R). The prototype's
pellet is 0.20R, but the simulation exports 0.295R: this fixture deliberately
shows the production size. The separate S2 prism fixture uses the production
0.62R simulation size at a base radius of 8.1px. Fifteen compact physics snapshots expose boost trails
without requiring simulation or AI changes.

### Fragment support and geometry contract

`snake.frag` now owns the literal `BOUNDS_*` constants. Rust reads those literals
at compile time in `shader/bounds.rs`; extrusion, UV scale, viewport culling,
and wrap selection use the same values. This adds no runtime parsing, allocation,
vertex storage, or draw calls. Effect UVs are body radii rather than normalized
quad coordinates: expanding the impact quad preserves spark speed and width.

Sprite AA ceilings are 0.04 effect-coordinate units, 0.2 head units, and 0.8
food units (minimum 0.008). Food's pixel-dependent ornament width is bounded
to 0.6 food units. These ceilings retain normal one-pixel softening. Body AA
keeps its original derivatives and is contained by the compact ribbon fade. Previously neither
had a finite support bound under arbitrary supported projections. The impact's
Gaussian flash now smoothly ends at 7 body radii (the old quad boundary), rather
than having infinite mathematical support. Body light fades over the last 0.04
body radii of the interpolated extrusion, which also contains curvature-limited
ribbons and shrinking corpse pieces while preserving their tube-width payload.

The table uses conservative maxima, includes the new AA ceiling where relevant,
and lists quad half-extents. Body values are multiples of the local taper times
body radius; head values are head radii; food values are exported food size.

| Content | Shader support before → after | Previous quad | Current quad |
|---|---:|---:|---:|
| Body outer halo + breathing/rounded taper | <=2.230 → <=2.230 | 2.2102 | 2.5 |
| Body shadow / softened inner glow | Unbounded AA → <=local quad edge (2.5) | 2.2102 | 2.5 |
| Body traveling light waves | <=2.489 → <=2.489 | 2.2102 | 2.5 |
| Body sheen/chevrons/saddles/spine lights | Exterior AA unbounded → contained by tube alpha | 2.2102 | 2.5 |
| Curvature-limited body / corpse dissolution | Shader exceeded extrusion → compact edge fade | Curvature-clipped | Same curvature limit, compact support |
| Corpse displacement | <=1.6 → <=1.6 body radii from original edge | Width + 1.6 wrap margin | Width + shared 1.6 wrap margin |
| Head tube/shadow/smooth neck | Unbounded AA → x<=1.831, y<=1.468, rear>=-1 | x=[-1,2.8], y=1.5 | x=[-1,2.9], y=1.75 |
| Head glow (outer / softened inner) | 1.584 / unbounded AA → <=1.636 front/side | y=1.5 | y=1.75 |
| Eye sockets/iris/blinks/look/pupils | Exterior AA unbounded → inside head skin | Same head quad | Same head quad |
| Eye glow incl. hunting/leader/flare | Unchanged: x=[-0.65,1.65], abs(y)<=1.71 | y=1.5 | y=1.75 |
| Tongue incl. full flick + AA | Unbounded AA → x<=2.865, abs(y)<=0.465 | x<=2.8 | x<=2.9 |
| Crown outline / additive glow | Outline AA unbounded → x=[-0.98,1.02], abs(y)<=1.035 | Same head quad | Same head quad |
| Boost bow strokes incl. AA | x AA unbounded → x> -2.244, x<2.191, abs(y)<=2.4 | x>=-1.2, y=2.5 | x>=-2.3, y=2.5 |
| Contrail | Unchanged: exact half-width 0.465*r*u; no longitudinal displacement | Same ribbon | Shared 0.465, same ribbon |
| Kill/leader ring + AA | Unbounded AA → <=6.364 body radii | 7 | 7 |
| Boost ring + AA | Unbounded AA → <=3.564 body radii | 7 | 7 |
| Kill flash | Unbounded Gaussian before; compact <=7 now | 7 | 10.2 (shared impact quad) |
| Kill sparks + stroke/AA | Endpoint <=9.744, unbounded AA → <=10.108 body radii; endpoint <=9.744 | 7 | 10.2 |
| Spark food halo / twinkle cross / core | 4.6 / 4.5 / unbounded AA → 4.6 / 4.5 / <=1.85 | 4.6 | Shared 4.6 |
| Essence shard halo / rotated rhombus | 4.4 / unbounded AA → 4.4 / <=3.307 | 4.6 | Shared 4.6 |
| Spent pellet halo / core | 3.0 / unbounded AA → 3.0 / <=1.8 | 4.6 | Shared 4.6 |
| Prism glow / seed ring / fuse / ripe halo / rays / core / pop | 4.6 / <=3.84 / <=4.16 / <=2.89 / <=4.01 / <=1.96 / <=4.6 | 4.6 visual radii | Shared 4.6, with prism display factor 1.6 |
| Food pulse / expiry | Unchanged: modulates amplitude, never size | 4.6 | Shared 4.6 |
| Vacuum streak | Unchanged width/length; explicit start gate: half-width max(0.65*s,0.5px), length s*(3+8*attraction) | Endpoint bounds + width | Shared 0.65, same bounds |
| Developer steering | Unchanged width/length; explicit start gate: half-width 1.2px; length max(7*r,52) projected per axis | Endpoint bounds + width | Shared 1.2, same bounds |

Head wrap bounds are the radius of the furthest quad corner, computed from the
shared forward and side extents. Body/corpse bounds include extrusion and drift;
food bounds include both vacuum endpoints and the greater of halo/streak width.
Contrails and developer overlays use both endpoints plus their shared width.
All retain six vertices per primitive, including each selected arena copy.

The independent analytic regression derives spark trajectory extrema, both bow
polynomial bounds, breathing/taper quantization, head ornament extents, and
rotating food shape bounds. Integration tests compare those supports to emitted
vertices and UV scale, sweep all 256 effect-age bytes, and check the newly
covered impact edge/corner copies. Existing independent-tiling tests cover every
primitive, anisotropic projections, corpses, and arenas spanning multiple copies.
The allocation regression continues to exercise growth, effects and resets.


## R3 active effects

Surge/Phase use the existing body effect bits without adding ribbon vertices.
Surge starts a three-edge rim arc every two segments with 30% probability,
reseeded at 18 Hz, with the prototype's 1.2px stroke and 85% brightness. Phase
fades tube/head coverage to 45% with a +/-6% flicker and adds a moving dashed
outline and scan bands. Head
PHASED is derived from the live effect record for rendering; corpse/expired
records never retain an active look. Classic fallback tints Surge's existing
outline, fades Phase with an accent outline, and draws a dashed Magnet ring.
Palette accents retain the item system's mono/pastel adjustments.

Sprite kind **15** is the Magnet ring: coordinates in body radii, exact 9r reach,
three rotating compact 1.1r spark halos, and a 10.2r quad extent from the shared
GLSL/Rust bounds contract. It costs six vertices per visible copy. Transients,
expiry warnings and Magnet rings share the eight-visible-quad cap in that
priority order. Warning amplitude now remains visible through expiry instead
of fading away as the counter approaches zero. Existing food vacuum streaks
already match the prototype's `size * (3 + attraction * 8)` length; the mechanics'
9r attraction reach exposes those streaks throughout the Magnet ring.

Every new procedural shader animation reads the existing frozen `time` uniform;
classic active effects similarly freeze their time under reduced motion. Wrap
bounds, including tiny arenas and anisotropic projection, are checked against
independent wall-bounded tiling. A 1000-frame active-effect regression builds
both render paths with zero allocations.

The native capture test has a **powerups** row with a boosted Surge and its
expiry warning, Magnet with twelve incoming food streaks, and Phase crossing a
solid rival. Capture it on the real desktop:

```sh
QT_QUICK_BACKEND=rhi QSG_RHI_BACKEND=opengl \
  SNAKES_CAPTURE_PATH=/tmp/snakes-r3.png \
  build-vis/bin/test-snakerenderer captureShaderFixture:powerups
```

The row saves `/tmp/snakes-r3.png.powerups.png`. Ordinary CTest skips captures
unless the capture path is set; the fixture's active payload and vertex budgets
are separately tested with the software backend. `benchmarkPowerupGeometry`
adds active effects to the same 14-snake/400-food mature benchmark fixture.

The custom material commits the immutable R8 atlas in `updateSampledImage`,
before Quick reads its RHI texture. Returning the wrapper alone leaves the
texture uncreated/unuploaded; a successful pipeline probe does not test that
upload lifecycle. The first commit creates/uploads the atlas, and subsequent
calls return immediately without allocation. The capsule glow uses the
prototype's 3.4R / 0.32 halo and 0.5R / 0.8 orbiting spark.

`bakedShaderResourceLayout` checks reflected uniform binding 0, every std140
member offset against the material's upload layout, and the sole atlas sampler
at binding 1. Every successful shader capture also requires at least six bright
pixels in each capsule's central icon square, excluding the rim, halo and spark.
Run the capture row on both GLES (`QSG_RHI_BACKEND=opengl`) and Vulkan
(`QSG_RHI_BACKEND=vulkan`) to validate native texture upload and sampling.


Worker verification: Release build and full CTest in `build-vis` passed **14/14**
in 91.74 s, including **441 Rust test executions**, unchanged Classic parity,
and the new active-effect allocation/bounds cases. Three CPU-8 samples of
100000 iterations measured median geometry **0.02963 ms** for the staged
foundation baseline, **0.02930 ms** for final inactive (-1.11%), and
**0.03039 ms** for final active (+2.56%), within the 5% gate. The active mature
fixture uploads **11094 vertices / 266256 bytes**, versus **11064 / 265536**
without active effects. Raw paired logs, exact commands and the worker report
are in `build-vis/bench-*.txt`, `benchmark-active.sh` and `WORKER_REPORT.md`.
Real-GPU appearance remains for the orchestrator's desktop capture.

The visual-defect follow-up passed full CTest **14/14** in 80.86 s. A fresh
normal-scheduler paired benchmark (three 100000-iteration samples) measured
**0.02833 ms** baseline, **0.02866 ms** inactive, and **0.02928 ms** active:
+3.35% versus the fresh baseline and -1.18% versus the historical 0.02963 ms
baseline. CPU-8 was contended during the first pinned attempt; both sets of
logs and exact commands are retained in `build-vis/WORKER_VISUAL_FIX_REPORT.md`.
Native GLES/Vulkan recaptures remain for the orchestrator; this sandbox cannot
create their display/context. Each capture now checks the central icon pixels.


## Render advisory classes

Copy bounds cover complete emitted geometry: classic capsule hex strokes include
both pixel minimums, and pickup/expiry rings include their outer stroke. Shader
capsule support joins the compile-time GLSL bounds contract. Sibling primitives
(food/highlights/vacuum, body/eyes/crown, contrails, death flashes, steering,
warning and Magnet rings, shader heads/corpses/impact/succession/boost rings) retain
their full geometry bounds and independent tiling/analytic regressions.

Both paths use one eight-visible-copy effect budget. Transients walk shared
history newest first, then warnings, then Magnet. Classic death flashes now use
the same history as pickup/expiry rings; offscreen geometry and expired history
consume no budget. Triangle count does not affect a logical copy's cost, and
required vertices count even when the caller's buffer needs a retry. Shader
impact, succession and boost rings obey this same budget; classic does not emit
succession or boost rings. Capsule birth rings are part of persistent items.

Traveling waves retain their origin even after replacement or expiry of the
active effect. Each body sample chooses the strongest retained wave (newest wins
ties); classic tints each edge from that origin. Shader vertices encode the
origin separately from active-effect bits. The vertex shader converts it to
premultiplied RGB light before interpolation, preventing interpolated bit fields
from changing colours/flags between wave origins. Kill and boost light remains
white, crown light remains gold, and pickup light keeps its originating accent.
The vertex ABI stays 24 bytes; no vertices, draw calls or per-frame allocations
are added. A retained origin byte array is touched only when different-colour
waves coexist. Single-colour waves retain the original brightness-only emission,
with a constant origin in the body payload (zero-strength light is suppressed
in the vertex shader).

## S1: ABI-v3 faces and races (0.11.0 step B)

Head vertices retain the 24-byte layout and six-vertex quad. On heads only,
`params.z` bits 1..4 now store Calm=0, Sleepy=1, Hunting=2, Scared=3, Angry=4,
Happy=5, Trapped=6, Dizzy=7, Frozen=8. Bit 0 is Boost, 5 is Phase, 6 is Leader, 7 is jaw bit 2.
Body flags and wave-origin decoding are unchanged. `params.y` has the tier in
bits 0..1, mood intensity in 2..5 (0..15), white crown in 6, pickup flare in 7.
`params.w` has signed pupil offsets in two three-bit fields (codes 0..6,
neutral 3, ranges +/-0.18 and +/-0.35), then the low two bits of a three-bit jaw opening. Happy uses
jaw=7 for the first 27-tick blep; Sleepy maps its 39-tick yawn to a sine-shaped
opening with seven visible steps. The blink/tongue seed now derives from colour instead of occupying
intensity bits. Compatibility snapshots without moods still derive hunting,
trapped and frozen looks from flags; production uses the exported mood.

Kind 16 is a screen-aligned emote: one six-vertex quad per live owner (half-extent `max(1.3r,12.5px)*pop`),
maximum three, never duplicated across seams. The renderer checks generation,
alive status, glyph and age against the persistent frame snapshot. A 5.4-tick
back-ease pop, hold and nine-tick fade fill the 45-tick life. Calm removes the
pop and shortens life/fades, yawn and blep to 60%. Snapshot ages advance
even when the procedural shader clock is frozen. The entire quad is monitor-clamped and
excluded from `clockRect` with a one-pixel guard. A clock relocation must stay within twice the bubble half-extent of the
monitor-clamped centre; otherwise the bubble is omitted. `snakes_core_render_set_clock_rect` adds a setter without
changing any ABI-v3 records or vertex layouts. The QML clock supplies local
monitor coordinates independently of seamless arena offsets.

Atlas tiles 0..4 retain the shipped icons; 5/6 remain reserved for future
Flip/Whirlpool icons; 7..11 contain !, ?, four anger brackets, Zz and heart,
generated by the same build-time R8 signed-distance script. New accent tints
use 88% grey on Mono and 30% white on Pastel; mood colours are grey on Mono.

Kind 11 reuses birth byte 128..255 for the 30-tick landing telegraph, and
0..127 for the legacy 15-tick birth ring (already complete at the end of
landing, rather than replayed). Target/crosshair/ghost hex/icon
all use its existing capsule quad. Kind 17 is a six-vertex contest arc in
capsule-radii UVs: byte y is closeness, z direction, w leader/contested/track bits (1/2/4). Exactly one contender quad per
item draws a full 1px track at strength .10, owned by the leader or the first
contender when no leader is flagged.
It uses the exported two contenders/ETAs, wrapped world distance and projected
direction. Radius is 1.62 capsule radii; half-angle .35+.85c, alpha .5+.5c,
stroke 2.0+2.4c pixels (leader 1.15x, follower .9x; follower alpha .8x).
Lifted palette colours mixed with 25% white and a compact squared halo
keep dark colours readable. Quad half-extent is `max(3.4r,1.62r+12px)`.
The leader is brighter; close ETAs pulse at 1.1 Hz.
Pixel-minimum stroke bounds select the same item seam copies even at tiny
projections. Calm keeps arcs static. Classic omits bubbles/arcs, uses eye
colours/pupil sizes for moods, and a faint ghost/ring during landing.

The capture's **faces** and **faces-mono** rows write directly to `SNAKES_CAPTURE_PATH`. Columns
are Calm, Sleepy (yawning), Hunting, Scared, Angry, Happy (blep), Trapped, Dizzy,
Frozen; top row hatchlings (~22px high), middle adults. Five emotes appear above
the first three hatchlings and two adults. Bottom left is landing, bottom centre is a close contested race
(two additional hatchlings at 6r and 8r on opposite sides, c=.912/.853), bottom right
a contested pair. Two bounded fixture snapshots expose all eighteen mood samples, two close contenders and
five glyphs without exceeding the production caps in either snapshot.

```sh
QT_QUICK_BACKEND=rhi QSG_RHI_BACKEND=opengl \
  SNAKES_CAPTURE_PATH="$HOME/.cache/agent-scratch/plasma-wayland-screensaver/snakes-s1.png" \
  build-s1/bin/test-snakerenderer captureShaderFixture:faces
QT_QUICK_BACKEND=rhi QSG_RHI_BACKEND=opengl \
  SNAKES_CAPTURE_PATH="$HOME/.cache/agent-scratch/plasma-wayland-screensaver/snakes-s1-mono.png" \
  build-s1/bin/test-snakerenderer captureShaderFixture:faces-mono
```

Repeat with `QSG_RHI_BACKEND=vulkan`; the test requires both renderers to use
shader geometry and checks that capsule icons and all five emote glyphs have
visible atlas pixels. Real-GPU legibility and pacing remain desktop checks.
S1 measurements and the distinction between the historical >11k stress
fixture and the matched ~8.6k spec/chaos fixtures are in `R11_STEP_B_REPORT.md`.

## S2 prism sprites, gulps and Feast colours

PrismSeed reuses the existing six-vertex kind-8 quad, displayed at 1.6x the
simulation size. A white pearl grows from .35 to .8 visual radii inside a
spectral information ring at 2.7 radii, filling clockwise from the top with a
white fuse spark. Ripe fruit has an iridescent pearl, spectral halo at 1.75 radii,
eight alternating dispersion rays, and a stronger glow. Its ripe-age byte drives
one expanding ring over 1 s plus orb overshoot. The shock stroke is bounded
inside the quad even at tiny display scales. The halo drains over the final 5 s;
a 1 s fade replaces expiry blinking. Colours are independent of the snake palette:
Mono is silver and Pastel mixes 30% white. The 12-byte fallback scales its discs
and draws twelve seed-ring segments, two per hue-table entry.

Gulps scale existing normals in a band of about thirteen body samples per gulp;
Gaussian width is `1 + .35 exp(-((i-center)/2.1)^2)`. Overlaps take the maximum.
Feast colours interpolate a six-entry hue table over about 22 body samples:
a two-segment front, five full-strength head-side segments, then an exponential
wake cut at 20 segments. Hue spans 11 segments and drifts continuously at 0.3 cycles/s.
Scratch colour alpha carries up to 0.6 light strength; the shader merges it with
other wave lights, using origin zero for the vertex's own hue. The wake uses the
Feast event's retained `duration_ticks`, matching the gulp even
when meal growth or boost payments change body length. Each overlapping Feast
keeps its own duration. The wake fades for 0.5 s after the bulge reaches the tail.
Both paths
add zero primitives for gulps and rainbow, retain allocated scratch and preserve
vertex counts. The ordinary shader ribbon retains its original hot loop and calling signature;
gulps and rainbow use a separate feature path. Mono body accents retain 12% chroma;
Pastel mixes 30% white. Calm freezes halo/ray rotation and body hue drift, retains
seed information, removes orb overshoot, and shortens the pop, gulps and wake by
40% (0.3 s tail fade). The frozen procedural clock controls rotation, pulse and
hue drift; event lifetimes use at least the advancing simulation clock in Calm.
This also applies to retained pickup/kill waves, head flares, transient rings,
corpse fades and contrails. Bubbles, jaws, landing, ripening and expiry retain
their authoritative snapshot ages/countdowns. Calm rays use a fixed 0.2 rad offset.
Prism contest arcs
sit outside the seed ring and rays at 3.3048 visual radii, reuse kind 17 and
share the existing six-arc scene cap. There are no new flashing signals:
halo hue turns at 0.12 rev/s, rays at 0.25 rad/s, and glow twinkles at ±15%, 0.35 Hz.

Capture on an accessible desktop (no servers needed):

```sh
QT_QPA_PLATFORM=wayland QT_QUICK_BACKEND=rhi QSG_RHI_BACKEND=opengl QSG_RENDER_LOOP=basic \
  SNAKES_CAPTURE_PATH="$PWD/build-s2/prism.png" \
  build-s2/bin/test-snakerenderer captureShaderFixture:prism
QT_QPA_PLATFORM=wayland QT_QUICK_BACKEND=rhi QSG_RHI_BACKEND=opengl QSG_RENDER_LOOP=basic \
  SNAKES_CAPTURE_PATH="$PWD/build-s2/prism-mono.png" \
  build-s2/bin/test-snakerenderer captureShaderFixture:prism-mono
```

Both prism rows use real proportions: seeds at 0/33/67/100%, a ripe fruit beside
a Surge capsule and sparks, two committed hatchlings racing, and three gulps
(hatchling, adult, violet) with Happy/Heart. `REAL_SHIFT` offsets ripe/gulp start
ticks (negative advances age; e.g. -30 settles the pop), `REAL_LIFE` sets the ripe
life byte (0..255), `REAL_CALM=1` freezes the presentation clock, and
`REAL_BOOST=1` overlays boost waves. No `REAL_FIXTURE` switch is needed.
The S2 chaos fixture adds two active gulps and a Feast wave
without extra vertices. Hardware capture/pacing still requires a live GPU session.

## S3 Venom

`venom.rs` reserves full prior-frame trails and two 800-point orphan slots at
construction. Compact tail history can deliver Sever without replacing those
trails; generation, reset and geometry changes invalidate stale copies. The
cut's original taper is retained. Orphan shader edges use the live-body contract:
extrusion `r*BODY*width`, across UV `width`, and taper in colour alpha. The pattern
tier follows the victim's post-cut length; `along` counts original source segments
so resampling cannot stretch chevrons. A 60% white snap fades over 1/8 s, then the
piece uses its palette colour and the ordinary corpse boost during dissolve.
A 256-entry sine table drives lateral wriggle with amplitude
`r*sqrt(1-p)*(0.4+0.6*j/(len-1))`. A 100-edge-per-orphan LOD plus a global
200-primitive budget bounds added shader vertices to 1200; cut-end glow copies
are reserved from that same budget before emitting edges, then drawn over them.
Calm suppresses wriggle but holds the piece for the full sim-provided duration
(33 ticks), matching shard release, then dissolves over 0.6*0.55 s.
Both shader and 12-byte fallback paths draw the
orphan, stump, acid spine and strike. Mono/Pastel use existing accent transforms.
The fallback orphan half-width remains `r*width`, without the shader envelope.

Shader kinds 23 and 24 reuse the ordinary head quad for Venom and STRIKE without
repacking mood/pupil fields. Bite impact reuses kind 6 and the eight-effect cap.
Sprite kind 25 `ACID_GLOW` uses payload `[25,0,0,0]`, UVs in +/-1 and colour
alpha as its strength. It adds a palette-aware radial acid glow at the orphan's
cut end (2.4r, strength 0.6 fading through the hold) and the live stump (2.2r,
strength 0.5 fading over 48 ticks); seam copies follow the normal sprite bounds.
Sever also starts an acid wave on the biter, reusing the existing two wave slots
and body vertices just like the white kill wave.
Capture rows `venom` and `venom-mono` use a 1920x1080 arena, default r=8.1 and
capsule radius 2.1r, and replay an intact trail before showing the cut.
`VENOM_AGE=<ticks>` advances the capture after Sever (for example 20 for wriggle,
34 for dissolve); `REAL_CALM=1` freezes the procedural clock while age advances.

## Stage E: monitor-filling giants

`examples/render_giant.rs` measures the C render ABI with a rounded rectangular
spiral: 6000 segments, radius 18, physical spacing 21.24px, plus eleven 80-segment
snakes. Every giant source point fits inside a 3440x1440 monitor. The shared-world
case is 7920x1440, with viewports (0,0,2560,1440), (2560,0,3440,1440) and
(6000,195,1920,1080); the giant fills the middle viewport. Each timed build
advances frame history, and buffers/renderer scratch are allocated before timing.
Output reports mean/p99 build ms, visible giant source points, vertices, written
bytes and caller capacity bytes. `reverse` reverses viewport/path order; `orphan`
measures a fresh 2999-segment Sever and a cached draw separately.

```sh
RUSTC=/usr/bin/rustc RUSTDOC=/usr/bin/rustdoc /usr/bin/cargo build \
  --manifest-path rust/snakes-core/Cargo.toml --frozen --offline --release \
  --example render_giant --target-dir build-render-e/render-target
/home/mjstanton/.local/bin/heavy taskset -c 8 \
  build-render-e/render-target/release/examples/render_giant
/home/mjstanton/.local/bin/heavy taskset -c 8 \
  build-render-e/render-target/release/examples/render_giant reverse
/home/mjstanton/.local/bin/heavy taskset -c 8 \
  build-render-e/render-target/release/examples/render_giant orphan
```

The single-monitor build averages about 0.103ms in both paths on CPU8: shader
41280 vertices / 990720 bytes, classic 105366 / 1264392 bytes. The 0.6ms giant
build gate has ample headroom; live-body LOD/culling changes are unnecessary.
The existing 9600-vertex chaos gate is unchanged. Qt's retained geometry grows
geometrically, requiring four allocations across 1600->6000 growth; it retains
its storage on later draws. Simulation export history already reserves 7120
segments per V2 snapshot and doubles on exceptional overflow. Rust allocation
tests cover first builds, growth, Sever, cached orphan draws and reprojection.

Uniform 100-edge sampling of a 2999-segment tail cuts up to 218px across this
spiral's corners. Long-only cached best-first selection reduces that to 9.02px,
retaining head/tail, bends, taper and original indices for markings/dissolve.
Selection targets 0.75px until the unchanged 100-edge budget is exhausted; the
fixed orphan cap takes precedence over that target. The short (<=800 segment)
orphan path is retained exactly. One giant orphan emits 606 shader vertices
(including its cut glow); the complete isolated Sever frame has 612 shader / 672
classic vertices, within the existing shared 1200-vertex orphan cap. Fresh
selection costs about 0.17ms, cached builds about 0.0074ms shader / 0.0043ms classic.

Capture rows `giant`, `giant-classic`, `giant-venom` and `giant-venom-classic` use
real 3440x1440 proportions, with optional `VENOM_AGE` and `REAL_CALM` controls:

```sh
QT_QPA_PLATFORM=wayland QT_QUICK_BACKEND=rhi QSG_RHI_BACKEND=opengl \
  QSG_RENDER_LOOP=basic \
  SNAKES_CAPTURE_PATH="$HOME/.cache/agent-scratch/plasma-wayland-screensaver/giant.png" \
  build-render-e/bin/test-snakerenderer captureShaderFixture:giant -o -,txt
```

Repeat with the other row names and distinct output paths; Vulkan also supports
these rows. The worker sandbox denies display sockets, so real RHI capture must
be verified outside that sandbox. A review aid can rasterize classic triangles
at native proportions without a display (flat average vertex colours, without
GPU colour interpolation or shader effects):

```sh
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  SNAKES_CAPTURE_PATH="$HOME/.cache/agent-scratch/plasma-wayland-screensaver/giant-raster.png" \
  build-render-e/bin/test-snakerenderer captureGiantClassicRaster:giant -o -,txt
```

Use `captureGiantClassicRaster:giant-venom` for the severed tail. Rust giant
fingerprints freeze both full-body formats and keep collision detail unchanged.

Shader viewport work uses conservative separating planes before preparing a
wall-bounded snake. Both previous/current endpoints and boost trail samples
participate; the padded envelope includes heads, gulps, corpse drift and stump
glow. History, races, bubbles and independent effects still advance. Normal
preparation skips a point only when both incident live-body edges are wholly
outside the padded viewport. Wrapped bodies and corpse normals retain their
original walks. Food rejects offscreen sprite/streak bounds before packing its
colour and phase. Retained geometry is byte-identical to the unculled path.

Exact taper profiles for lengths 1–512 are initialized once by Renderer/Spatial
construction and shared read-only across snakes, AI and viewports (1.13 MiB).
Short-body growth reads these profiles; it never allocates or repeats powf.
The long-body and giant binning paths keep their existing caches. A complete
width/alpha-byte comparison guards the shared profiles against their original
formula.

`scripts/prepare-snakes-frame-benchmark.py` generates an external CMake driver
for `tests/perf_snakesframe.cpp`: actual simulation/export/history and shader
QSG geometry for three independent view clocks. The instrumented simulation is
a scratch copy; production receives no timers. `--paced` measures the first
three minutes and minutes 10–15 at wall-clock presentation rates, accelerating
only the unmeasured middle. `--hash` checks every geometry byte separately from
timing. This harness excludes the Qt render loop, GPU and Wayland draw.

S4: body `params[1] & 128` means FROZEN, independent of the three-bit held-effect
kind. CPU colour mixes 60% toward palette-adjusted #c8eeff, including Feast
vertices. The body shader draws spine-aligned diamonds every two segments
(0.84 w long × 0.40 w wide) and four-point glints on about 12% of segments,
reseeded at 3 Hz with a smooth envelope. Frozen heads exhale three growing
near-white puffs every 1.25 s, contained within the existing 2.9-unit head quad.
Kind 18 is one Nova quad, sized to 1.05 times the event's world radius: a crisp
near-white front expands cubically, with 18 tapering needles, a cold wake,
inner glow and static frost flecks. Kind 6 with `params[2]=1` is the thaw crack:
seven bright shards burst from outside the head to about 5.5 snake radii,
with a thin pop ring and short frost puff. Its quad uses `BOUNDS_FROST_CRACK`
(6 snake radii), shared by shader and Classic geometry and checked in bounds
tests. Both effects share the eight-effect copy budget and store lifetimes
and age in simulation time (Nova 0.7 s, crack 0.5 s), including Calm.
The 12-byte fallback uses a whitened ice ring/spikes and longer outward crack
strokes with the same cubic expansion. Frozen snakes crawl and interpolate;
Calm suppresses shiver and breath, retaining static crystals and glints.

## Inventory (ABI v4, release 0.18)

`inventory.rs` retains three motions per snake and three capsule drop origins;
no frame/tick heap storage is added. Pips (kind 26) use screen-aligned 0.85r hex
sprites at neck arc distances 2.2r/4.3r/6.4r. Their bytes are item kind, rim life,
and flight progress (128..255); alpha remains opaque through Phase/Night/Frost.
Each snake emits its pips immediately after its head. A 42-visible-copy cap
(three slots times MAX_SNAKES) bounds held, flying and fizzling gems, including
seams. All 14 snakes can display their three slots away from wrap seams; seam
copies share the budget. The S6 inventory chaos ceiling is 10,236 vertices.

Stash/Use/Fizzle event clocks preserve stream-order slot compaction and age on
simulation time in Calm. Store flight shrinks from 2.47x over .22s; the settle
pop is absent in Calm. Wind-up follows the neck to .35r ahead of the head over
four ticks, and draws last among its own pips. Stash/fizzle use collapsing kind
13 rings; activation still uses the existing kind 12 ring and body wave. Item
bubbles pass ABI glyphs 5..9 directly to kind 16, mapping to atlas tiles 0..4.
Death capsules interpolate from their neck slot to the authoritative drop
position and grow over .12s. A compact tail-only history defers origin recovery
until a full corpse snapshot is available.

Surge uses kind 28 on the existing widened head quad, four bounded kind 27
slipstream ribbons (up to 14 edges each), and the existing tail history ring
while cruising as well as boosting. Contrails are Surge yellow at .35/.55;
the 18 Hz crackle is removed. Slipstream shimmer is +/-22% at about 1.9 Hz and
freezes in Calm. Magnet's kind-15 byte w is opening progress (1..255), with 255
also used for full reach in Calm. Nova front delivery delays victim icing only
in presentation; simulation freeze remains immediate.

Phase enters over .1s using body effect code 6 and head kinds 29..255, without
extra vertices or a vertex ABI change. The kind byte carries head fade while
alpha retains the neck taper. During entry the body origin bits encode a
3-bit fade (interpolant location 6); the wave keeps its Phase accent. The head
uses the same fade interpolant, decoded independently from the taper. Normal Phase retains code 3.
Classic fallback keeps opaque hex gems and procedural icons, including flights,
fizzle/compaction and drops, using the existing triangle sink without allocation.

Neck position and rainbow colour caches compare their actual inputs. A changed
neck point/radius or event age/palette/wave invalidates them; Classic rendering
invalidates shared rainbow scratch. Buffer retries and unchanged redraws reuse
samples without repeating square roots or colour exponentials.

Inventory history distinguishes zero-duration touch Use from held completion.
Stash clears the destination slide clock, and configuration Fizzles have a bounded
independent event key/queue so same-tick retries and a cleared next export retain
their animation. Compact tail-only Nova history defers victim attribution until
full head coordinates are available. Resting inventory uses the existing body
broadphase; only stash/fizzle transients require admission outside that envelope.
Surge budgets count visible ribbons and seam/cull margins bound the widened
normals, including the maximum 35% feeding pulse.

# Snakes mechanics golden traces

Run from any directory:

```sh
tests/parity/snakes/record.sh
# Verify that a fresh recording is byte-identical, without replacing fixtures:
tests/parity/snakes/record.sh --check
```

Requires Qt **6** QML/QtTest modules, `/usr/lib/qt6/bin/qmltestrunner`, Python 3,
GNU `timeout`, and Bash. Set `QMLTESTRUNNER` to another Qt6 runner if needed.
The script sets `QT_QPA_PLATFORM=offscreen`, `QT_QUICK_BACKEND=software`, and
`QML_XHR_ALLOW_FILE_READ=1`; it reads the frozen JavaScript `oracle/Snakes.qml` and its sibling dependencies
from commit `c846506`, independently of the native production visual. Its optional first argument is the output directory.
Temporary recordings live under this directory and are removed on exit. There
are no servers, timers advancing physics, or background workers. The QML item
has `simulationDriver=false`, `reducedMotion=true`, and a no-op native renderer;
`stepSimulation` is called synchronously at 30 Hz. The exception is the focused
vacuum trace at 240 Hz and the explicit collision-only trace described below.

Load XZ-compressed or plain JSON with the supplied standard-library loader:

```python
# Add tests/parity/snakes to sys.path.
from fixtures import load_fixture
trace = load_fixture("tests/parity/snakes/fixtures/default-wrap.json.xz")
```

`python3 tests/parity/snakes/fixtures.py FILE` pretty-prints either format.
XZ compression uses Python stdlib LZMA preset 9 and stores no timestamp or filename. Long fixtures and respawn declare
`encoding="coordinate-delta2-v1"`: only segment x/y and food x/y are stored as
integer second differences of coordinates quantized to 1e-6. The loader restores
them to the frame schema below. Predictor state is keyed by (snake slot, segment
index) or food ID, separately for x and y, and starts with previous coordinate
and previous difference zero. For each recorded frame, encode `q=round(x*1e6)`,
`d=q-previous`, `encoded=d-previousDifference`; update both histories. Decode
adds the last difference, then the last coordinate, then divides by 1e6.
Histories persist through gaps/deaths and are updated only for present entries.
No timestamps, real values or events are omitted by encoding. The converter
checks encoded/decoded frame equality before publication. Other focused fixtures
are ordinary readable JSON without this encoding. All fixture bytes
are deterministic on the same Qt/runtime. Floating trigonometric results may
have platform-specific last-bit differences; recorded outputs round real
numbers to six decimal places. `initialState` retains full JS precision.

## Isolation and instrumentation

Qt6 QML method properties are read-only from JS. `tst_record_snakes.qml` reads
the frozen oracle using a synchronous local XMLHttpRequest, renames only the
instrumented function declarations to `parityOriginal_NAME`, and appends
wrappers dispatching through a writable `parityHooks` property. It creates the
result with `Qt.createQmlObject` using the **oracle source URL** so its frozen
`VisualUtils.js` and `FrameClock.qml` resolve correctly. No production file is
modified. `sourceSha256` hashes those three frozen oracle inputs under their historical
production-path keys so existing fixtures remain byte-identical. Declaration
renaming preserves original mechanics bodies and redirects their internal
calls through instrumentation too.

The exact AI entry points disabled are:

- `updateSnakeBrains`: replaced with assignment of scripted desired angles to
  all living snakes, with no cooldown, brain work, hazard or food queries.
- `planSteering`, `chooseGoal`: replaced with throwing guards. Reaching either
  fails recording rather than silently restoring AI.
- `applyCollisionSafety`: no-op; no desired-angle or recovery changes.
- `applyWallSafety`: returns false; no anticipatory wall turn.

No remaining movement path invokes planning, candidate evaluation, goals,
lookahead reservation, self escape or brain state machines. Passive
`analyzeFoodClusters` remains in `stepSimulation` unchanged: it annotates food,
consumes no RNG, and cannot steer without a planner. `snakeIntelligence=75`
remains relevant to the **mechanical turn-rate limit**, not heading selection.
`rush` and recovery start at zero and have no AI producer.

The other wrappers (`random`, `makeSnake`, `consumeFoodParticle`,
`markCollisions`, `explodeSnake`, `moveSnake`) invoke the preserved original
function exactly once. They count draws or observe events. `makeSnake` also
assigns scripted `desiredAngle` immediately on birth/respawn (the normal step
orchestration creates a respawn after its brain-update phase). `moveSnake`
verifies that growth retains the old neck as segment 2 and the old tail as the
last object. No mechanics RNG values are substituted or skipped.

## Steering scripts and scenarios

`desiredAngle(tick, index, snakeState)` is defined in `Recorder.js`. Angles are
normalized using production `normalizeAngle`. The state argument is available
but the current script deliberately does not inspect it. For long runs:

```
phase = tick / 30
quarter = floor(tick / 90) % 4
angle = index * 2.399963229728653
      + 0.7 * sin(phase * 0.8 + index)
      + quarter * pi / 2
```

For focused runs desired angle is zero, except the second head-on/seam snake
uses pi, the body owner's angle is -pi/2, and vacuum-lock requests zero before
tick 2 then pi. This also applies at tick 0 and to respawns. The script ids are
`weave-quarter-turn-v1` and `focused-table-v1`.

| Fixture | Ticks | Setup / asserted outcome |
| --- | ---: | --- |
| default-wrap | 900 | Seed 1, 1280x720, defaults below, wrapping |
| deadly-walls | 900 | Same, deadly walls enabled, safety disabled |
| self-collisions | 900 | Same wrapping world, self collisions enabled |
| maximum-density-trails | 900 | 3440x1440, density=100, trails=100, wrapping |
| head-on-equal | 4 | Two length-20 snakes, both die with reason head |
| head-on-length-difference | 4 | Lengths 24 and 20, only the shorter dies |
| body-hit | 8 | Right-moving head hits rival's vertical body |
| self-exclusion | 2 | Collision-only: own segments 1..9 at head are excluded; own segment 10 at head kills |
| feeding-growth-insertion | 24 | Eats value 2.2, inserts at least two segments behind head |
| vacuum-lock | 40 | dt=1/240; captures food at 2.7 radii; head is repositioned beyond capture range, still eats |
| death-burst-food | 8 | Head beyond deadly wall, emits edible corpse food |
| respawn | 190 | Same initial wall death, runs through respawn |
| wrap-seam-crossing | 8 | Head starts at width-1, crosses seam and lives |
| wrap-seam-collision | 4 | Opposing heads at width-5 and 5 collide across seam |
| world-resize | 12 | Tick 5 resizes 1280x720 to 1920x900 before stepping |

Defaults: animationSpeed=100, animationDensity=50, animationScale=100,
animationPalette=spectrum, trailAmount=35, snakeIntelligence=75,
snakeSelfCollisions=false, snakeDeadlyWalls=false, monitorBehavior=seamless.
Virtual/screen origins are zero. Default context produces nine snakes and 82
ambient particles; maximum settings produce fourteen snakes and 183 ambient
particles. All seeds are 1.

Focused runs initialize the normal world first, reuse the first one/two real
snake records, remove other snakes and food, then construct documented geometry
in `setup`. Consequently their tick-0 RNG already includes the entire initial
world's draws. Standard focused snakes have radius/baseRadius=6,
speedBias=1, birthLength=length, 7.08 segment spacing; all other hidden fields
come from production initialization. Their exact constructed state, including
trail history, speed bias, IDs and food values, is in `initialState`.
Ambient replenishment stays enabled; focused traces are real full mechanics
steps except `self-exclusion`. That kernel trace calls scripted angle assignment,
`markCollisions`, and `explodeSnake` at dt=0 without food update or movement.
All own segments 1..9 coincide with the head on the first pass.
Before its second pass segment 10 is explicitly set to [500,350] with identical
previous position; its `setSegment` event is an input action. In `world-resize`,
the `resize` event is also an input action, applied before `stepSimulation`.
In `vacuum-lock`, tick 2 moves the head to [540,370] before stepping, asserts the
particle is already locked and is now beyond the original capture radius, and
requests a turn toward pi. Its `setHead` event is an input action. This isolates
persistent capture ownership without replacing production vacuum/feeding.

## Schema (version 1)

Each file is one JSON object containing:

- `version`, `id`, `seed`, initial `world:[width,height]`, `ticks`, `dt`,
  `script`, `operation`, `config`, and `sourceSha256`.
- `initialState`: the **unrounded complete production simulation snapshot**
  after the focused setup and tick-0 script assignment, for initializing a Rust
  replay. Includes segments/previous positions, trail points and counters.
- `frames`: tick 0 and every tick for focused setups. For long runs: tick 0,
  every tick 1..300, then 310,320,...,900 (361 frames per run).

Each frame contains `tick`, `simulationTime` (post-operation, seconds),
`rngState` (exact uint32 stored as JSON number), cumulative `rngDraws` since
scenario initialization, current `world`, `nextFoodId`, `nextFeastId`, `snakes`,
`food`, and `events`.

Snake objects contain `index` (stable slot ID across respawns), `alive`, `angle`,
`desiredAngle`, `radius`, `length`, `growth`, `growthStretch`, `respawn`, and
`segments:[[x,y],...]`. Dead snakes have length 0 and empty segments. Coordinates
and all real output values are rounded with JS `Math.round(value*1e6)/1e6`.
Food order is the actual engine array order (feeding traverses it backwards).
To keep fixtures small, each food entry is a tuple in this exact column order:

```
[id, x, y, size, vacuumOwner, attraction]
```

`vacuumOwner=-1` means unlocked; a nonnegative snake slot is a capture lock.
`attraction` is pull strength. Food nutrition, velocity, lifetime, attraction
targets, and passive annotations are present in the full initial snapshot, not
repeated in every frame. Locked particles in the engine have life=-1.

Events appear in production execution order and always include their actual
`tick`. A sampled frame includes **all** events since the previous emitted
frame, so unsampled long-run events are not lost:

- `spawn` / `respawn`: `snake` slot.
- `eat`: `snake`, `food` particle ID, `value`.
- `death`: `snake`, production `reason` (wall/head/body/self), `killer`,
  `killerCandidates`, `emittedFood` (original explodeSnake return value).
- `growth`: `snake`, resulting `length`.
- `resize`, `setSegment`, and `setHead`: explicit replay actions, described above.

The engine does **not** expose a killer ID. The observer uses production world
and swept geometry helpers to derive candidate owners from collision-time
positions before any explosions. Self killer is the same slot; wall killer is
null; a unique other owner becomes `killer`. Multiple eligible owners leave
`killer=null` and are all listed in `killerCandidates`. This is observational
attribution, not a claim about which grid occupant the engine encountered first.
A Rust comparison should enforce production reason/death state and compare
killer only when unique. Sampling cannot restart a simulation from each frame:
only `initialState` carries complete trail/previous-position state.

## Exact remaining RNG draws

LCG is `state=(state*1664525+1013904223)%4294967296`; result is state/4294967296.
Initialization sets state to `(abs(seed)+1)*2654435761%4294967296`, minimum 1.
The wrapper counts every production `random()` invocation, including setup:

- `makeSnake`: radius and length (2 draws); `spawnPosition` initial fallback
  angle (1), then candidate x/y/angle (3 per attempt, 1..48 attempts depending
  on geometry/clearance); speedBias, turnBias, wanderPhase, aggression,
  brainCooldown (5). Total **8 + 3*attempts** per snake. The latter four are
  AI-related birth attributes, but their initialization draws remain to preserve
  the current mechanics RNG stream. Respawns use the identical function.
- `addAmbientFood`: x, y, value, color, lifetime (5), then `addFood` phase (1):
  **6 per ambient particle**. Includes initial population, replenishment after
  eats and expires (at most three additions per step).
- `addFood`: **1 phase draw**, including explicitly inserted focused food.
- `explodeSnake`: angle, force, x jitter, y jitter, value, lifetime (6), then
  addFood phase (1): **7 per emitted death particle**, plus **1 respawn-delay
  draw**, even if no particles can be emitted.

No draws remain in steering, movement, growth, vacuum/feeding, collision tests,
food motion, cluster analysis or resize. Disabling AI removes its execution
entirely; no compensating draws are made. `rngState` and `rngDraws` jointly make
stream consumption inspectable at each sampled tick.

## Determinism check

```sh
tests/parity/snakes/record.sh tests/parity/snakes/.verify-a
tests/parity/snakes/record.sh tests/parity/snakes/.verify-b
diff -rq tests/parity/snakes/.verify-a tests/parity/snakes/.verify-b
rm -r tests/parity/snakes/.verify-a tests/parity/snakes/.verify-b
```

`record.sh` publishes output only after all QML scenario assertions pass and the
converter verifies 15 complete fixtures, complete event coverage, and total
size below 3 MiB. It fails after 180 seconds if the runner hangs. The recording suite is self-contained; CTest replays the published fixtures
without Qt recording or JavaScript AI.

Verified on Qt 6.10.2: two independent full recordings each passed QTest
(3 passed, 0 failed); all 15 fixture files were byte-identical. Published fixture
size is 2,402,420 bytes. Loader/schema/sampling/time/source-hash checks passed.

Deliberate limits: no AI parity, no presentation/accumulator/Canvas parity, no
cross-platform bitwise floating-point promise, and no definitive killer for
multi-owner collisions. The focused growth assertion checks object identity
inside the recorder; the fixture itself records the resulting coordinate/length
history. Standard feeding, ambient spawning and passive cluster annotation
remain coupled to full steps rather than being artificially disabled.

## Rust replay

```sh
/usr/bin/cargo build --frozen --offline --release \
  --manifest-path rust/snakes-core/Cargo.toml --features parity --example parity
python3 tests/parity/snakes/compare.py \
  --binary rust/snakes-core/target/release/examples/parity
ctest --test-dir build-parity --output-on-failure -R snakes-parity
```

The `snakes-parity-build` CMake target builds the example with the existing
SnakesCore toolchain/profile/environment into the build directory's Cargo target
directory. The `snakes-parity` CTest entry compares all 15 fixtures. No downloads,
JSON crate, production public API, controller changes or C ABI changes are needed.
`world/golden.rs` is not used as the replay oracle.

`compare.py` converts each fixture to protocol version 1: whitespace-separated
numbers, one per line. The order is version, Config fields, tick count, dt,
collision-only flag, initial RNG state/draw count/time/IDs/growth budget, snake
count and complete mechanics records (including previous positions and active
trail points), food count and complete mechanics records, then commands for
every tick. A command contains a sample flag, action count and actions
(1=resize, 2=setHead, 3=setSegment), and the normalized desired angle for each
slot. The Python encoder and Rust decoder explicitly declare the field order.
No expected post-initialization outcomes enter the runner: sample ticks and the
three documented input actions are the only data read from expected frames.

The binary imports the full-precision initial snapshot once, advances every
tick through the shared production mechanics, and emits unrounded JSON lines
only at recorded sample ticks. Events accumulate across unsampled ticks.
Killers are observed before any explosions, just as in the JS recorder.
Respawns receive the tick's scripted angle without movement until the next tick.
The collision-only trace uses the actual collision/explosion kernels; the vacuum
trace uses its recorded dt. Instrumentation is compiled only with `parity` and
is opt-in, so even feature-enabled ordinary worlds retain zero tick allocations.

All discrete outcomes are exact, including sample RNG state and draw count,
event order/tick/reason/victim/killer candidates, growth lengths, respawns, food
IDs/array lengths/capture owners and next IDs. Continuous values must be finite
and differ by at most **1e-6** across **every** sample, including the complete
900-tick runs. This is stricter than allowing late chaotic divergence. Output
reports maximum absolute error and its location, maximum through tick 300, and
the first sample with any discrepancy. The suite grants no automatic roundoff
exception: any such future divergence must be investigated before changing
policy. Fixtures quantize outputs to six decimals, so an otherwise exact replay
can have an error of 5e-7.

The numerical mechanics needed no fixes for these recordings. The Rust default
palette size was corrected from seven to six entries to match every JS palette;
the replay explicitly configures six. There are no intentional behavior
differences in the tested mechanics. AI, rendering, time accumulation and unrecorded settings
remain outside this proof.

See [RESULTS.md](RESULTS.md) for per-fixture errors, exact verification commands,
the configuration fix and paired benchmark measurements.

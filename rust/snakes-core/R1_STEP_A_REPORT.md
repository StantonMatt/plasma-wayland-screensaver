# R1 step A: boost and ABI groundwork

Reference source: `e35791f988099f8536c766e9ed01a00b1b6842dd` (0.7.3).
Baseline crate sources were copied to `/tmp/snakes-r1-baseline` before edits;
there was no imported code, fixture regeneration, commit, push, or review loop.
The pre-existing untracked `docs/` design files were left intact.

## Behaviour and compatibility

- Rust `Config.rules` explicitly selects `Classic` or `V2`. Rust's default stays
  Classic for source compatibility. FFI `rule_set=0` defaults to V2, `1` selects
  Classic, `2` selects V2. Unknown values and nonzero config reserved fields fail
  validation. Changing rule sets restarts the world.
- V2 requests (`rush>0`) start immediately: 24 ticks at rush 0.6, then 36 ticks
  of cooldown. The normal turn-rate formula and additive 0.2 digestion boost
  remain. At initiation length must be at least 12; paying may reduce it below
  12. Cost is captured at initiation: `2 + length / 100` whole segments.
  Cumulative payments at burst tick `t` are `floor(cost * min(t,21) / 21)`.
  Each removed tail position becomes a 0.5-value, 8-second pellet. Pellet phase
  is a hash of food ID, with no RNG draw. At the food cap a pellet replaces the
  oldest food so every paid segment is represented.
- A frozen active burst cuts to cooldown. Effect fields/kinds and the PHASED
  flag are reserved for future items; there is no item spawning or power-up
  mechanic in this step. Item exports are empty.
- Dead V2 bodies remain in their original segment storage for 17 exported tick
  states (death frame and the next 16); then disappear. They cannot collide,
  eat, count against live segment budgets, or enter AI hazards. Classic deaths
  keep the old immediate clearing behaviour.
- Leader eligibility is length >=30. The current eligible leader keeps the
  crown until a rival is >=3 segments longer; only one leader is exported.
  Kill and succession events are implemented; sever/pickup/nova are reserved.
  Event overflow evicts oldest, events reset per tick, repeated exports do not
  consume them, and multi-tick stepping retains only the final tick's events.
- AI changes only translate old rush >=0.5 into ready fixed boost requests and
  remove dodge/crossing rush in V2. HUNTING comes from prey/coil state; TRAPPED
  uses the existing selected rollout's reachable area, below 1.5x the existing
  turnaround-area requirement when uncapped. Budgeting and future burst-duration
  rollout modelling remain step B. `Controller::intent_flags` and
  `World::set_intent_flags` provide hooks.
- C++ snapshots retain event/item buffers; event capacity is reserved at 32.
  The existing renderer skips corpses and otherwise ignores the new fields.
  No visual redesign or release/package version change was made in step A.

## ABI v2 layout (x86_64, C-compatible)

All offsets below are bytes. Sizes and key offsets are asserted in Rust and in
both C and C++ through `include/snakes_core.h`. `u8/u16/u32/u64` mean unsigned
fixed-width integers. Explicit reserved fields are zero on export.

| Record | Size / alignment | Fields at offsets |
|---|---|---|
| config | 80 / 8 | width f64@0, height@8, density@16, trails@24, scale@32, speed@40, intelligence@48; seed i32@56; palette_size u32@60, self_collisions@64, deadly_walls@68, rule_set@72, reserved@76 |
| steering_input | 24 / 8 | id u32@0, generation@4, desired_angle f64@8, rush f64@16 |
| snake | 56 / 8 | id u32@0, generation@4, alive@8, color_index@12; radius f64@16, angle@24, desired_angle@32; segment_offset u32@40, segment_count@44, flags@48; effect_ticks u16@52, effect_kind u8@54, boost_ticks u8@55 |
| segment | 16 / 4 | x f32@0, y@4, previous_x@8, previous_y@12 |
| food | 48 / 8 | id u64@0; x f32@8, y@12, size@16, phase@20, attraction@24, attraction_x@28, attraction_y@32; color_index u32@36, kind u8@40, life_fraction u8@41, reserved u16@42; trailing ABI padding 44..47 |
| item | 24 / 8 | id u64@0, x f32@8, y@12, kind u8@16, reserved_byte u8@17, age_ticks u16@18, life_ticks u16@20, reserved u16@22 |
| event | 32 / 8 | tick u64@0, x f32@8, y@12, snake_id u32@16, other_snake_id@20, color_index@24, kind u8@28, reserved u8[3]@29 |
| frame_sizes | 24 / 4 | snakes u32@0, segments@4, food@8, items@12, events@16, reserved@20 |
| frame_info | 40 / 8 | tick u64@0, simulation_time f64@8, world_width@16, world_height@24, geometry_generation u64@32 |
| statistics | 56 / 8 | alive u32@0, total_segments@4, food@8, reserved@12; deaths u64@16, wall_deaths@24, head_deaths@32, body_deaths@40, self_deaths@48 |
| ai_debug_point | 8 / 4 | x f32@0, y@4 |
| ai_debug_record | 200 / 8 | id u32@0, generation@4, target_count@8, path_count@12, flags@16, reachable_cells@20; safe_seconds f64@24, target_food_ids u64[5]@32, path ai_debug_point[16]@72 |

Snake flags: BOOSTING=1, COOLDOWN=2, HUNTING=4, TRAPPED=8, FROZEN=16,
PHASED=32, LEADER=64, CORPSE=128. Food kinds: spark=0, shard=1, pellet=2,
prism=3. Event kinds: kill=0, sever=1, pickup=2, nova=3, succession=4.
Effect/item kinds are reserved: none=0, Surge=1, Magnet=2, Phase=3, Venom=4,
Frost=5. `boost_ticks` includes the latest executed tick. `life_fraction` is
0..255; vacuum capture retains the existing locked-lifetime mechanics.

`snakes_core_export_frame` retains its function arguments, using larger v2
records. New `snakes_core_export_extras(world, items, item_capacity, events,
event_capacity)` writes the extra arrays without allocation. Both calls validate
all capacities/pointers before writing any output. All handle calls must remain
serialized. Frame segment counts include corpses; statistics count only live
segments. Kill events identify victim then first rival owner; wall/self uses
`UINT32_MAX` for the owner. Succession identifies new then old leader, or
`UINT32_MAX` for no predecessor. Tick is the completed simulation tick; initial
succession may be tick zero. Event positions scale with world resize, including
wrapped ring entries. Renderers should deduplicate events by frame tick and reset
event caches on world restart/geometry-generation changes.

## Verification and measurements

Distribution rustc/cargo 1.93.1, CMake 4.2.3, Release builds. Every full build,
full suite, and benchmark matrix was run via `/home/mjstanton/.local/bin/heavy`.
The matrix uses 3440x1440, maximum density/trails, self collisions, three seeds
(73, 20260814, 991), IQ100/IQ50, deadly/wrap, eight simulated minutes per case
(14,400 ticks). Runs use shared-host wall time without CPU pinning.

Commands (paths are relative to the worktree unless absolute):

```sh
cmake -S . -B build-r1 -DCMAKE_BUILD_TYPE=Release -DBUILD_TESTING=ON
/home/mjstanton/.local/bin/heavy cmake --build build-r1 -j 4
/home/mjstanton/.local/bin/heavy ctest --test-dir build-r1 --output-on-failure
/home/mjstanton/.local/bin/heavy env RUSTC=/usr/bin/rustc RUSTDOC=/usr/bin/rustdoc /usr/bin/cargo test --frozen --offline --release --manifest-path rust/snakes-core/Cargo.toml
/home/mjstanton/.local/bin/heavy env RUSTC=/usr/bin/rustc RUSTDOC=/usr/bin/rustdoc /usr/bin/cargo run --frozen --offline --release --manifest-path /tmp/snakes-r1-baseline/Cargo.toml --example ai_scorecard -- 8 --ai-only
/home/mjstanton/.local/bin/heavy env RUSTC=/usr/bin/rustc RUSTDOC=/usr/bin/rustdoc /usr/bin/cargo run --frozen --offline --release --manifest-path rust/snakes-core/Cargo.toml --example ai_scorecard -- 8 --ai-only
/home/mjstanton/.local/bin/heavy env RUSTC=/usr/bin/rustc RUSTDOC=/usr/bin/rustdoc /usr/bin/cargo run --frozen --offline --release --manifest-path /tmp/snakes-r1-baseline/Cargo.toml --example bench_mechanics
/home/mjstanton/.local/bin/heavy env RUSTC=/usr/bin/rustc RUSTDOC=/usr/bin/rustdoc /usr/bin/cargo run --frozen --offline --release --manifest-path rust/snakes-core/Cargo.toml --example bench_mechanics
cc -std=c11 -Wall -Wextra -Werror -I rust/snakes-core/include rust/snakes-core/tests/ffi_smoke.c build-r1/cargo-target/Release/release/libsnakes_core.a -lpthread -ldl -lm -o /tmp/snakes-r1-ffi-smoke
/tmp/snakes-r1-ffi-smoke
git diff --check
```

Focused checks used the same `RUSTC`/`RUSTDOC` environment and
`/usr/bin/cargo test --frozen --offline --manifest-path rust/snakes-core/Cargo.toml`:

- `--lib world::v2_tests`: first run passed five tests and failed an overly
  strict floating-point position comparison. Wrap normalization can differ by
  one ULP; the comparison was fixed and all six passed in the combined run below.
- `--lib v2_tests`: seven mechanics/FFI tests passed at that stage.
- `--lib v2_requests_only_fixed_ready_boosts_and_exports_intent`: passed.
- `--lib leader_hysteresis_round_trips_through_ffi`: passed.
- `--test allocation zero_allocations_v2_boost_pellets_events_corpses_and_ffi_export -- --nocapture`:
  passed, zero allocations and all three pellet/event/corpse paths observed.
- `--lib world::tests::maturity_turn_radius_length_penalty_rush_and_radius_cap`:
  passed (Classic).
- An attempted `--lib world::golden` matched zero tests; the actual
  `world::tests::golden` tests passed in the full suite.

Initial native build failed because the parity fixture loader needed an explicit
new food kind initializer. That was fixed without changing fixture contents.

### Mechanics benchmark

| Rule/build | Mean ms | p50 us | p95 us | p99 us | Max us |
|---|---:|---:|---:|---:|---:|
| Unmodified Classic | 0.027269 | 28.262 | 34.509 | 41.521 | 171.343 |
| V2 step A | 0.028408 | 28.457 | 34.216 | 42.175 | 177.467 |

Mean increase: **0.001139 ms**, below the +0.05 ms gate.
Both runs ended with 622 deaths, 8 live snakes, 254 live segments and 442 food.
The BaselineController requests no boosts, so this isolates counter/leader/event/
corpse overhead; boost-inclusive behaviour is measured by the scorecard below.

## Changed files

- Simulation and observations: `src/world.rs`, `src/world/query.rs`,
  `src/controller.rs`, `src/lib.rs` (all under `rust/snakes-core/`).
- ABI and native integration: `rust/snakes-core/src/ffi.rs`,
  `rust/snakes-core/include/snakes_core.h`, `src/snakesimulation.h`,
  `src/snakesimulation.cpp`, `src/snakerenderer.cpp`.
- Minimal AI adaptation and documentation: `rust/snakes-core/src/ai/mod.rs`,
  `src/ai/attack.rs`, `src/ai/tests.rs`, `src/ai/README.md` (the latter three
  paths also under `rust/snakes-core/`).
- Compatibility and tests: `rust/snakes-core/src/world/golden.rs`,
  `src/world/parity.rs`, `src/world/v2_tests.rs`, `tests/ai_ffi.rs`,
  `tests/allocation.rs`, `tests/ffi.rs`, `tests/ffi_smoke.c`,
  `tests/generate_js_golden.py` (all except the next file under
  `rust/snakes-core/`), and `tests/test_snakescore_abi.cpp`.
- Measurement/handoff: `rust/snakes-core/examples/ai_scorecard.rs`,
  `examples/bench_mechanics.rs`, `examples/support/accounting.rs` (under
  `rust/snakes-core/`), and this report.

The native build passed. Existing C++ fixture aggregates emit missing-field
initializer warnings for the appended v2 fields; C++ value-initializes these
omitted fields to zero. Production configuration initializes the new fields
explicitly. The strict C smoke test is warning-free.

### Final verification results

- Fresh Release native build: passed.
- Fresh full `ctest`: **14/14 passed**, 62.53 seconds. Includes renderer,
  simulation, QML, overlay, ABI, Rust, Classic parity and fixture recording.
  No recorded fixture changed.
- Fresh full Release Cargo suite: **317 test executions passed**
  (98 library, 1 AI FFI, 101 allocation/includes, 1 controllers, 2 FFI,
  100 observer/includes, 14 tooling; no failures).
- Strict C ABI smoke test, relinked against the final archive: passed.
- Allocation tests: zero allocations through boosted cap stepping and FFI
  snapshots with observed pellets, events and corpses; existing allocation/AI
  cap tests also passed.
- `git diff --check`: passed.

The focused `--lib resizing_scales_the_entire_wrapped_event_ring` regression
passed and verifies RNG state is unchanged. Final build/test gates were run
sequentially in one finite script under `heavy` (600-second timeout per command):

```sh
/home/mjstanton/.local/bin/heavy bash /tmp/snakes-r1-final-checks.sh
# Script runs, sequentially:
cmake --build build-r1 -j 4
ctest --test-dir build-r1 --output-on-failure
env RUSTC=/usr/bin/rustc RUSTDOC=/usr/bin/rustdoc /usr/bin/cargo test --frozen --offline --release --manifest-path rust/snakes-core/Cargo.toml
```

No temporary server or watcher was started; all finite task processes exited.
Final source is uncommitted.

### Eight-minute AI scorecard (before → V2)

| Seed | IQ | Walls | Exact kills | Self deaths | Wall deaths | Mean ms | p99 ms |
|---|---:|---|---:|---:|---:|---:|---:|
| 73 | 100 | deadly | 12 → 9 | 6 → 0 | 0 → 0 | 0.2164 → 0.2026 | 0.4191 → 0.3721 |
| 73 | 100 | wrap | 19 → 4 | 3 → 6 | 0 → 0 | 0.2385 → 0.2237 | 0.4719 → 0.4077 |
| 73 | 50 | deadly | 12 → 12 | 7 → 1 | 0 → 0 | 0.2163 → 0.2157 | 0.3939 → 0.3627 |
| 73 | 50 | wrap | 12 → 8 | 4 → 0 | 0 → 0 | 0.2471 → 0.2158 | 0.5088 → 0.3548 |
| 20260814 | 100 | deadly | 10 → 10 | 4 → 2 | 0 → 0 | 0.2188 → 0.2161 | 0.3816 → 0.3763 |
| 20260814 | 100 | wrap | 11 → 5 | 2 → 5 | 0 → 0 | 0.2493 → 0.2318 | 0.5193 → 0.4281 |
| 20260814 | 50 | deadly | 13 → 10 | 6 → 1 | 0 → 0 | 0.2227 → 0.2037 | 0.4160 → 0.3632 |
| 20260814 | 50 | wrap | 14 → 7 | 4 → 1 | 0 → 0 | 0.2518 → 0.2174 | 0.4982 → 0.3649 |
| 991 | 100 | deadly | 11 → 7 | 3 → 1 | 0 → 0 | 0.2205 → 0.1900 | 0.3955 → 0.3190 |
| 991 | 100 | wrap | 14 → 8 | 3 → 1 | 0 → 0 | 0.2431 → 0.2234 | 0.4694 → 0.4139 |
| 991 | 50 | deadly | 14 → 7 | 4 → 2 | 0 → 0 | 0.2156 → 0.2241 | 0.3734 → 0.4774 |
| 991 | 50 | wrap | 14 → 4 | 3 → 0 | 0 → 0 | 0.2391 → 0.2294 | 0.4698 → 0.4365 |

Totals: exact opponent kills **156 → 91**, self deaths **49 → 20**, wall deaths **0 → 0**, all deaths **205 → 111**. All ambiguous-kill counts are zero.

Equal-weight mean tick time across all 12 cases: **0.231600 → 0.216142 ms** (delta **-0.015458 ms**). Largest individual-case increase: **+0.0085 ms**, below the +0.05 ms gate.

The full R1 aggression target is **not met by step A**: kills fell with the paid boost rule. Aggregate self deaths improved, but IQ100/wrap
regressed at seeds 73 (3 → 6) and 20260814 (2 → 5). This is the measured input for the explicitly separate step B (boost budgeting, burst-duration forecasting, attacks/escapes/food races and coil admission), not a release-ready aggression result. No broader AI changes were made to chase that target in step A.

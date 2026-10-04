# R1 step B — paid boost planning

V2 uses a single observed/requested burst schedule throughout cutoff refinement,
physical safety rollouts, rival prediction and sampled prey replies. The schedule
keeps the observed nutrition reserve fixed, applies the real 21-tick tail-payment
schedule (including length/radius effects on motion), and ends the boost after
24 ticks. An already active burst decrements before the next forecast step.
Future food, new rival boosts and rival private controls are not predicted.
Classic motion retains its separate stage-speed behavior.

- Cutoffs start only with a ready boost and at least 16 segments, use fixed
  rush 0.6, and reserve the paid segments when checking the four-segment head
  advantage and deposited barrier length. Their utility pays approximately
  2.5 food for the first two segments, proportionally more for longer snakes.
  Retained attacks tolerate their own scheduled payments but cancel on growth,
  unrelated shrinkage, frozen state or changed motion.
- V2 admits nearby hunts at effective aggression above 0.2 and interest at
  least 0.75, retaining low-aggression foraging. The cutoff library considers
  both an early crossing and arrival-first crossing, at 18/24/36 ticks.
- Pursuit by itself spends no tail. Food-race requests require a target patch
  worth at least 3, a rival within 25% of our distance, and a forward approach
  with room for the boosted turning circle. A checked unboosted alternative
  competes with that request in ordinary ranking.
- Emergency boost is admitted only when every sampled unboosted control has
  less than 0.6 seconds of safety and a boosted control completes the horizon.
  The rollout uses physical speed/turn caps; its compound turn and exit control
  are retained when selected. Escape is allowed from length 12.
- V2 admits open/wrap and wall-side coils from length 150 and three times the
  victim's length: victim within 20 own radii, coil radius at least 4.2 own
  radii and the physical turning floor, and body for 1.2 circumferences. The
  ordinary rollout still checks entry, existing body, contraction and exit.
  Classic keeps the existing wall-U admission.
- HUNTING follows prey or active coil; TRAPPED uses uncapped reachable area
  below 1.5 times the existing turnaround requirement. These remain intent
  observations exposed through the step A hooks.
- Both burst schedules and the two additional candidate slots are preallocated.
  Identical fallback candidates reuse their area search. Tail-release bounds
  use the slowest forecast speed, so a current burst cannot prematurely release
  a body that remains after boost expiry.

## Eight-minute natural scorecard

Same 12 configurations as step A: maximum density/trails, 3440x1440,
self collisions, 14,400 ticks per run. Kills use exact opponent-owner events;
ambiguous kills are zero. Each triple is **0.7.3 / step A / step B**.
Timings compare the step A report with final step B, on the shared host without
CPU pinning. A fresh same-session step A matrix is summarized below too.

| Seed | IQ | Walls | Exact kills | Self deaths | Mean ms A → B | p99 ms A → B |
| --- | ---: | --- | ---: | ---: | ---: | ---: |
| 73 | 100 | deadly | 12 / 9 / 12 | 6 / 0 / 3 | 0.2026 → 0.2199 | 0.3721 → 0.3541 |
| 73 | 100 | wrap | 19 / 4 / 8 | 3 / 6 / 1 | 0.2237 → 0.2280 | 0.4077 → 0.4523 |
| 73 | 50 | deadly | 12 / 12 / 16 | 7 / 1 / 3 | 0.2157 → 0.2173 | 0.3627 → 0.3629 |
| 73 | 50 | wrap | 12 / 8 / 11 | 4 / 0 / 2 | 0.2158 → 0.2284 | 0.3548 → 0.4317 |
| 20260814 | 100 | deadly | 10 / 10 / 12 | 4 / 2 / 0 | 0.2161 → 0.2160 | 0.3763 → 0.3497 |
| 20260814 | 100 | wrap | 11 / 5 / 17 | 2 / 5 / 1 | 0.2318 → 0.2317 | 0.4281 → 0.4162 |
| 20260814 | 50 | deadly | 13 / 10 / 17 | 6 / 1 / 3 | 0.2037 → 0.2103 | 0.3632 → 0.3407 |
| 20260814 | 50 | wrap | 14 / 7 / 11 | 4 / 1 / 3 | 0.2174 → 0.2319 | 0.3649 → 0.3912 |
| 991 | 100 | deadly | 11 / 7 / 12 | 3 / 1 / 2 | 0.1900 → 0.2022 | 0.3190 → 0.3357 |
| 991 | 100 | wrap | 14 / 8 / 15 | 3 / 1 / 0 | 0.2234 → 0.2268 | 0.4139 → 0.4521 |
| 991 | 50 | deadly | 14 / 7 / 17 | 4 / 2 / 4 | 0.2241 → 0.2113 | 0.4774 → 0.3637 |
| 991 | 50 | wrap | 14 / 4 / 16 | 3 / 0 / 0 | 0.2294 → 0.2220 | 0.4365 → 0.4115 |

Totals: exact kills **156 / 91 / 164**; self deaths **49 / 20 / 22**;
wall deaths **0 / 0 / 0**. Every step B configuration has no more self deaths
than 0.7.3. Total deaths are 186. The requested aggression and self-safety gates
pass; step B has two more aggregate self deaths than step A.

Equal-weight mean tick **0.216142 → 0.220483 ms** against the step A
report, **+0.004341 ms**. Largest per-case increase is
**+0.0173 ms**.
All 12 means are below 0.25 ms, every increase is below 0.05 ms, and all p99s
are below 0.6 ms. A freshly rebuilt step A snapshot reproduces its **91 kills /
20 self deaths** and measures **0.220200 ms**; the same-session equal-weight
mean difference is **+0.000283 ms**. These wall-clock timings vary with
shared-host load; the measured gates pass, without claiming a noise-free CPU bound.

## V2 duels: step A → step B

The duel example now defaults to V2; `--classic` selects legacy free rush.
Both sides below use the same updated example against an untouched step A crate
snapshot and final step B. Eight scenarios × two IQs × two aggressions × ten
jitters × mirrors × role swaps: **1,280 worlds per mode**, first designated
death or 20 seconds. Each entry is high / low aggression kills per 40 worlds.

| Scenario | IQ | Responsive A → B (high / low) | Limited A → B (high / low) |
| --- | ---: | ---: | ---: |
| open_crossing | 100 | 0 / 0 → 0 / 0 | 31 / 18 → 27 / 18 |
| wall_escape | 100 | 0 / 0 → 0 / 0 | 0 / 0 → 4 / 0 |
| wrap_escape | 100 | 0 / 0 → 0 / 0 | 11 / 3 → 0 / 3 |
| equal_cluster | 100 | 0 / 0 → 0 / 0 | 0 / 0 → 1 / 1 |
| food_cutoff | 100 | 2 / 0 → 0 / 0 | 25 / 6 → 29 / 7 |
| long_encircle | 100 | 0 / 0 → 0 / 0 | 35 / 34 → 17 / 33 |
| feasible_crossing | 100 | 0 / 0 → 0 / 0 | 5 / 20 → 10 / 4 |
| infeasible_chase | 100 | 0 / 0 → 0 / 0 | 0 / 0 → 0 / 0 |
| open_crossing | 50 | 0 / 0 → 0 / 0 | 34 / 17 → 35 / 30 |
| wall_escape | 50 | 0 / 0 → 0 / 0 | 0 / 0 → 8 / 0 |
| wrap_escape | 50 | 0 / 0 → 0 / 0 | 22 / 4 → 0 / 4 |
| equal_cluster | 50 | 0 / 0 → 0 / 0 | 0 / 5 → 1 / 4 |
| food_cutoff | 50 | 0 / 0 → 0 / 0 | 13 / 25 → 12 / 27 |
| long_encircle | 50 | 7 / 0 → 0 / 0 | 38 / 40 → 19 / 40 |
| feasible_crossing | 50 | 0 / 0 → 0 / 0 | 17 / 12 → 20 / 26 |
| infeasible_chase | 50 | 0 / 0 → 0 / 0 | 0 / 0 → 0 / 0 |

Responsive: kills **9 → 0**; attacker deaths **5 → 9**; victim self deaths **0 → 0**; victim wall deaths **0 → 0**.

Limited: kills **415 → 380**; attacker deaths **23 → 40**; victim self deaths **0 → 0**; victim wall deaths **0 → 3**.

The retained `long_encircle` fixture starts 30 own radii from the victim
(radius 180, own radius 6), with 1,274.4 pixels of body versus 1,357.2 needed
for 1.2 loops. It fails the new 20-radius/1.2-loop admission. These original
fixtures remain unchanged for comparison; this scenario is not a positive
R1 coil benchmark. The open/wrap admission and physical safety paths are
covered by the focused regressions.

**Open release decision:** the natural matrix meets all stated numerical gates,
but responsive duel kills fall to zero and limited duel kills/deaths regress.
Limited prey is the unchanged diagnostic controller rather than the full AI;
its three wall deaths are excluded from opponent-kill attribution. No
forced-kill or duel improvement claim is made. Infeasible-chase kills remain
zero in both modes. These duel regressions remain unresolved and need a release decision.

## Final verification and reproduction

Distribution `/usr/bin/cargo`, `/usr/bin/rustc`, `/usr/bin/rustdoc` 1.93.1,
frozen/offline, Release. Every full suite, build and benchmark matrix ran under
`/home/mjstanton/.local/bin/heavy`. The final finite gate script ran sequentially:

```sh
/home/mjstanton/.local/bin/heavy bash /tmp/snakes-r1-step-b/final-checks.sh
# Exact inner commands (600-second timeout on each):
export RUSTC=/usr/bin/rustc RUSTDOC=/usr/bin/rustdoc
cmake --build build-ai -j 4
ctest --test-dir build-ai --output-on-failure
/usr/bin/cargo test --frozen --offline --release --manifest-path rust/snakes-core/Cargo.toml
/usr/bin/cargo build --frozen --offline --release --manifest-path rust/snakes-core/Cargo.toml --example ai_competition
rust/snakes-core/target/release/examples/ai_competition
rust/snakes-core/target/release/examples/ai_competition --scripted-prey
```

- Fresh native Release build: passed. `build-ai` was configured with
  `cmake -S . -B build-ai -DCMAKE_BUILD_TYPE=Release -DBUILD_TESTING=ON`.
- Fresh full CTest: **14/14 passed**, 70.85 seconds, including Rust, Classic
  parity/recording, native ABI, simulation, QML and overlay integration.
- Fresh full Release Cargo: **339 test executions passed**: 105 library,
  1 AI allocation, 1 AI FFI, 108 allocation/includes, 1 controllers, 2 FFI,
  107 observer/includes and 14 tooling. No failures.
- The new public V2 AI allocation test measures the first decision and 3,000
  ticks with observed boosts/hunts: **zero allocations/reallocations**.
  Existing cap, reconfiguration, boosted mechanics and FFI allocation tests pass.
- New physical regressions cover cost/expiry motion, tick-one rival prediction,
  admission/cooldown, food race/turn room, open/wrap coils, retained payment
  budgets, emergency escape and all three attack slots remaining boosted after
  crossing. The scratch-poison determinism regression now runs in both rules.
- `git diff --check`: passed. No commit, push, review loop, server or watcher.

Final matrix command:

```sh
/home/mjstanton/.local/bin/heavy env RUSTC=/usr/bin/rustc RUSTDOC=/usr/bin/rustdoc /usr/bin/cargo run --frozen --offline --release --manifest-path rust/snakes-core/Cargo.toml --example ai_scorecard -- 8 --ai-only
```

The immutable step A crate copy is `/tmp/snakes-r1-step-b/step-a`, copied before
AI edits (original 0.7.3 revision is `e35791f988099f8536c766e9ed01a00b1b6842dd`).
Only its duel example was updated to select V2 by default, matching the final
benchmark; its AI/mechanics stayed unchanged. Reproduce step A duels/matrix with
`/home/mjstanton/.local/bin/heavy bash /tmp/snakes-r1-step-b/baseline-benchmarks.sh`.
Raw artifacts under `/tmp/snakes-r1-step-b` include `matrix-5.txt` (final),
`matrix-step-a-paired.txt`, `duels-{step-a,final}.txt`,
`limited-{step-a,final}.txt`, `{build,ctest,cargo-test}-final.txt`, scripts and
intermediate matrix logs. All finite task processes exited.

Changed step B files: `src/ai/{mod.rs,attack.rs,pocket.rs,tests.rs,README.md}`,
read-only observation hooks in `src/world/query.rs`,
`examples/ai_competition.rs`, and `tests/ai_boost_allocation.rs` (all paths in
this crate). No mechanics, FFI/header, C++/renderer or version edits in step B.

Historical pre-R1 measurements are retained below.

## Historical pre-R1 AI — iteration 4

The advisory production follow-up fixes five accepted collision/feasibility
classes and their siblings, with unchanged mechanics/C ABI and zero steady-state
allocations. Against the previous README, exact opponent kills change
**182 → 156**, self deaths **39 → 49**, wall deaths stay **0**, responsive duel
kills change **4 → 5**, and mean step changes **0.2794 → 0.5451 ms (+95.1%)**
in separate sessions. Three same-session alternating normal-world pairs measure
**+0.2% mean / −6.0% median**. Self-safety, attack quality, oscillation and strict
performance acceptance remain open. All changes are uncommitted.

## Advisory production follow-up

These measurements supersede the rollout follow-up below. Source edits are
limited to `src/ai/{mod.rs,attack.rs,pocket.rs,tests.rs,README.md}`. No mechanics,
exports, C layouts, dependencies or versions changed.

| Accepted finding / class | Fix and siblings audited | Regression |
| --- | --- | --- |
| Attack-stage sweep bounds | Split collision intervals at the burst/crossing boundary. Body, losing/winning rival-head, rival-neck and deposited-self consumers share the same interval; broad phases use its traveled distance, and self-deposit padding retains the maximum stage bound. Covers the retained attack and both alternative slots. | `advisory_attack_sweeps_split_before_stage_speed_changes_for_all_attack_slots` |
| Skipped prey wall deaths | Check walls before any contact on every reply tick. The sibling current-self check also missed ticks, so it now sweeps every exact tick, alongside head and deposited-body contacts. Shared by all six reply controls. | `advisory_reply_wall_exit_is_terminal_even_when_it_returns_next_tick`; `advisory_reply_checks_current_self_geometry_on_previously_skipped_ticks` |
| Expired prey deposits | Shared distance-based trail clipping limits both prey self deposits and attacker barriers to their live body span, with distinct neck exemptions. Clips partially released edges too; preserves self-collision-off behavior. | `advisory_reply_releases_deposits_after_the_tail_passes_in_wrap` |
| Gaps between sampled barriers | Replace disconnected sampled points/chords with contiguous one-tick edges, including the latest deposited edge. The sibling prey-deposit point sampling uses the same exact edge helper. Broad-phase bounds precede interpolation and narrow checks; no curve approximation or per-tick allocation is added. | `advisory_reply_hits_barrier_between_former_sampled_intervals` (deadly and seam-wrapping arenas) |
| Pocket inner-radius floor | Check the proposed inner radius after subtracting pitch, including spiral curvature, before installing a pocket. Audited retained validation, entry, contraction and both goal-tracking rollouts; their existing floor checks remain consistent. | `advisory_pocket_admission_checks_the_inner_radius_before_installing` (radius 18, outer radius 145, pitch 41.4) |

All **six regressions fail before and pass after**. The isolated pre-fix crate in
`/tmp/ai4-prod-fix/before` receives only the new tests and a behavior-preserving
extraction of the old single-reply loop, exposing it to tests; its collision and
pocket logic stay unchanged. The existing positive wall-U entry, actual staged
motion, collision-setting and allocation regressions also pass.

### Natural scorecard: rollout follow-up → advisory fixes

Same 12 configurations, eight minutes each, consecutively on core 4, with no
concurrent task compiler, CTest or other task benchmark. Historical timings and
new matrix timings are separate sessions. Each arrow compares the previous
README's final value with this run.

| Seed / IQ / walls | Exact kills | Self deaths | Mean ms | p99 ms | Final oscillations/snake-min |
| --- | ---: | ---: | ---: | ---: | ---: |
| 73 / 100 / deadly | 12 → 12 | 6 → 6 | 0.2669 → 0.5492 | 0.4176 → 4.0704 | 11.25 |
| 73 / 100 / wrap | 21 → 19 | 4 → 3 | 0.3306 → 0.4919 | 0.6358 → 1.9976 | 9.40 |
| 73 / 50 / deadly | 18 → 12 | 1 → 7 | 0.2761 → 0.5298 | 0.5238 → 2.7702 | 8.35 |
| 73 / 50 / wrap | 15 → 12 | 2 → 4 | 0.2955 → 0.6111 | 0.4733 → 3.0286 | 9.78 |
| 20260814 / 100 / deadly | 16 → 10 | 6 → 4 | 0.2757 → 0.4300 | 0.4459 → 0.7657 | 10.70 |
| 20260814 / 100 / wrap | 19 → 11 | 2 → 2 | 0.2794 → 0.4660 | 0.4652 → 1.0013 | 8.45 |
| 20260814 / 50 / deadly | 9 → 13 | 5 → 6 | 0.2719 → 0.5435 | 0.6109 → 3.6119 | 9.71 |
| 20260814 / 50 / wrap | 15 → 14 | 2 → 4 | 0.2806 → 0.5274 | 0.5632 → 2.6201 | 9.19 |
| 991 / 100 / deadly | 9 → 11 | 3 → 3 | 0.2596 → 0.5511 | 0.4044 → 3.6352 | 12.77 |
| 991 / 100 / wrap | 17 → 14 | 2 → 3 | 0.2716 → 0.7069 | 0.4335 → 5.8002 | 8.38 |
| 991 / 50 / deadly | 19 → 14 | 1 → 4 | 0.2559 → 0.5822 | 0.5039 → 4.2305 | 10.74 |
| 991 / 50 / wrap | 12 → 14 | 5 → 3 | 0.2891 → 0.5522 | 0.4748 → 3.5973 | 8.83 |

Exact kills **182 → 156 (−26)**; self deaths **39 → 49 (+10)**; wall deaths
**0 → 0**. Bigger/smaller/similar kills are **115 / 34 / 7** with no ambiguous
masks. Deaths total **205**; opponent contacts account for **76.1%**. Hunting
kills **104 → 77** and staged kills **47 → 34** remain intention labels.
Natural runs still admit zero pockets. The self-safety gate regresses.

Mean step **0.2794 → 0.5451 ms (+95.1%)**; p99 range **0.7657–5.8002 ms**.
All twelve historical-session p99 comparisons exceed the 0.55 ms gate. The
large session difference is not a controlled attribution to these source edits.

Three alternating runs of 20260814 / IQ100 / deadly compare preserved pre-fix
and final executables, with identical observer code, configuration and core 4.
Each side repeats its own outcomes exactly.

| Pair / order | Pre-fix mean / p99 ms | Fixed mean / p99 ms |
| --- | --- | --- |
| 1 / before–after | 0.2823 / 0.4988 | 0.2978 / 0.5855 |
| 2 / after–before | 0.4347 / 0.9772 | 0.3906 / 0.8585 |
| 3 / before–after | 0.4154 / 0.7563 | 0.4464 / 1.2352 |

Paired arithmetic means are **0.3775 → 0.3783 ms (+0.2%)**; median means
**0.4154 → 0.3906 ms (−6.0%)**. These pairs do not establish a strict performance
non-regression bound across all worlds.

### Duels: rollout follow-up → advisory fixes

All eight scenarios, both IQs, jitters, aggressions, mirrors and role swaps:
**1280 worlds per mode**, unchanged first-death stopping rule. Values are
high/low aggression kills per 40 worlds.

| Scenario / IQ | Responsive high / low | Limited high / low |
| --- | --- | --- |
| open_crossing / 100 | 0 → 0 / 0 → 0 | 29 → 29 / 18 → 18 |
| wall_escape / 100 | 0 → 0 / 0 → 0 | 0 → 0 / 0 → 0 |
| wrap_escape / 100 | 2 → 2 / 0 → 0 | 36 → 36 / 3 → 3 |
| equal_cluster / 100 | 0 → 0 / 0 → 0 | 1 → 1 / 1 → 1 |
| food_cutoff / 100 | 0 → 0 / 0 → 0 | 23 → 23 / 20 → 20 |
| long_encircle / 100 | 0 → 0 / 0 → 0 | 26 → 26 / 31 → 31 |
| feasible_crossing / 100 | 0 → 1 / 0 → 0 | 7 → 7 / 20 → 20 |
| infeasible_chase / 100 | 0 → 0 / 0 → 0 | 0 → 0 / 0 → 0 |
| open_crossing / 50 | 1 → 1 / 0 → 0 | 34 → 34 / 17 → 17 |
| wall_escape / 50 | 0 → 0 / 0 → 0 | 0 → 0 / 0 → 0 |
| wrap_escape / 50 | 0 → 0 / 0 → 0 | 33 → 32 / 4 → 4 |
| equal_cluster / 50 | 1 → 1 / 0 → 0 | 0 → 0 / 0 → 0 |
| food_cutoff / 50 | 0 → 0 / 0 → 0 | 16 → 16 / 32 → 32 |
| long_encircle / 50 | 0 → 0 / 0 → 0 | 34 → 34 / 32 → 32 |
| feasible_crossing / 50 | 0 → 0 / 0 → 0 | 15 → 15 / 12 → 12 |
| infeasible_chase / 50 | 0 → 0 / 0 → 0 | 0 → 0 / 0 → 0 |

Responsive kills **4 → 5 (+1)**; attacker deaths **1 → 1**; victim self/wall
**0 / 0**. Limited kills **444 → 443 (−1)**; attacker deaths **31 → 31**;
victim self deaths **2 → 2**, wall deaths zero. Infeasible-chase kills remain
zero. These are sampled contacts, not forced-kill or enclosure proofs.

### Verification and artifacts

`RUSTC=/usr/bin/rustc /usr/bin/cargo test --frozen --offline --manifest-path rust/snakes-core/Cargo.toml`
passes **176 tests**: 85 library, 87 private allocation harness and four integration.
`cmake -S . -B build-prod -G Ninja -DCMAKE_BUILD_TYPE=Release -DBUILD_TESTING=ON`,
`cmake --build build-prod -j 6` and
`ctest --test-dir build-prod --output-on-failure` pass **14/14**, including
mechanics and recorded parity, native simulation, C ABI and QML integration.
`git diff --check` passes.

The Release 6000-segment / 450-food cap fixture retains 6000 segments with zero
allocations/reallocations over 1000 measured ticks plus 2000 reconfiguration ticks,
including first AI ticks. Its initial isolated timing is **0.8467 ms mean /
1.2933 ms p99**, against the previous README's **0.5098 / 0.6487 ms**.
Three alternating cap pairs use preserved pre-fix and final allocation test
executables on core 4, with no concurrent task compiler or benchmark. Every run
passes the zero-allocation assertions and retains all 6000 segments.

| Pair / order | Pre-fix mean / p99 ms | Fixed mean / p99 ms |
| --- | --- | --- |
| 1 / before–after | 1.1105 / 4.9449 | 0.9545 / 3.7109 |
| 2 / after–before | 0.9265 / 3.1543 | 0.9182 / 2.7479 |
| 3 / before–after | 1.0970 / 6.3944 | 0.8574 / 2.2767 |

Paired cap arithmetic means are **1.0447 → 0.9100 ms (−12.9%)**, median means
**1.0970 → 0.9182 ms (−16.3%)**. All three fixed cap samples are faster than their
paired baseline samples. The large timing variation and failed absolute 0.50 ms
cap still prevent a strict performance acceptance claim. Reproduce with
`python3 /tmp/ai4-prod-fix/cap_pairs.py`; the allocation executables are built with
`RUSTC=/usr/bin/rustc /usr/bin/cargo test --release --frozen --offline --manifest-path rust/snakes-core/Cargo.toml --test allocation zero_allocations_with_ai_at_caps_and_after_reconfiguration --no-run`
(and the isolated baseline manifest), then run sequentially with `taskset -c 4`.
`cap-{before,after}-[1-3].txt` and `cap-summary.json` retain the full results.

Reproduce the matrix, duels and alternating normal pairs with
`python3 /tmp/ai4-prod-fix/benchmarks.py`; its commands use immutable
`scorecard-before`, `scorecard-final` and `competition-final` executables on
core 4. Builds use `/usr/bin/cargo`, `/usr/bin/rustc`, Release, frozen/offline.
Raw logs, `summary.json`, the pre-fix crate, reproduction scripts and binaries
are preserved under `/tmp/ai4-prod-fix`: `regressions-before.txt`, `cargo.txt`,
`cmake-{configure,build}.txt`, `ctest.txt`, `release-build.txt`,
`allocation-release.txt`, `scorecard-<seed>-<iq>-<walls>.txt`, `duels-final.txt`,
`limited-final.txt` and `normal-{before,after}-[1-3].txt`.
No commit, push, review loop or persistent task process is created.

## Rollout feasibility follow-up

This follow-up fixes the coil finding as three related classes. The measurements
below supersede the previous review's results retained in the later sections.
No mechanics, exported functions or `repr(C)` layouts changed.

- **Candidate-specific gates:** only goal/coil-tracking candidates advance a
  spiral and apply its curvature/floor gate. Straight, ordinary turn, compound
  escape, harvest and flee alternatives still execute their physical safety
  checks. Attack eligibility and stage controls apply only to the retained
  attack and two attack slots. The straight-slot reuse optimization now rejects
  an attack continuation rather than silently copying its staged trajectory.
- **Checked publication:** candidates record whether a first physical step was
  evaluated, including rejected wall/body/head steps. Pre-step tactic rejections
  never enter selection. An ordinary straight rollout always supplies a checked
  fallback. When none completes the horizon, ranking prefers checked survival
  ticks, then reachable area, clearance and utility. Published heading and rush
  come from that same candidate; the Rust-only observer exposes `checked` without
  changing the stable C debug layout. Safe retained-plan reuse also rechecks the
  current first step and horizon.
- **Immediate retained-plan validation:** pocket validity runs every decision,
  before quota or response timing can defer tactics. It checks current speed,
  turning radius, lengths, pitch bounds, prey identity/position, release reserve,
  winding progress, hunt/pocket expiry, walls and recovery states. Generated
  pockets also retain their entry body length/radius and cancel on either
  change, so a shortened/resized body cannot reuse a stale release reserve.
  Crossing the
  spiral floor also cancels tracking. Every cancellation shares `clear_coil`,
  resetting retained heading/exit/turn/commitment/rush controls. Choosing a
  checked non-tracking continuation abandons the old coil immediately. Generated
  attacks retain their own motion/size snapshot and cancel immediately when
  speed, turn limits, length or radius changes; fresh cutoffs must pass the normal
  physical rollout. Harvest and flee store goals, not safety exemptions, and use
  the same current-geometry rollout as ordinary alternatives.

The sibling audit covers both tracking slots, straight-slot aliasing, both
attack alternatives, compound escape copies, retained-plan reuse, all-failed
selection, coil entry/contraction/cancellation, attack consumers, recovery,
harvest and flee. Six regressions cover gate isolation, delayed pocket
invalidation, exhausted fallback publication, straight/attack aliasing,
immediate attack invalidation and abandoning a pocket for a checked straight
continuation. All six fail on the pre-fix implementation in an isolated `/tmp`
copy and pass after fixing. The old copy receives only compile-compatible
private snapshot fields/helper and passive `checked` instrumentation; its gates,
validation and selection remain unchanged. Pocket invalidation exercises speed,
length (even a one-segment shrink that preserves eligibility), radius, hunt
expiry, three recovery states, wrapping, contraction floor,
pocket timeout and prey escape while tactical revision remains delayed.

### Natural scorecard: prior review → follow-up

Same 12 configurations, eight minutes each, consecutively on core 4; no task
compiler, CTest or other task benchmark runs concurrently. Timings compare
separate sessions; the same-session alternating comparison follows the table.

| Seed / IQ / walls | Exact kills | Self deaths | Oscillations/snake-min | Mean ms | p99 ms |
| --- | ---: | ---: | --- | --- | --- |
| 73 / 100 / deadly | 8 → 12 | 6 → 6 | 8.90 → 11.25 | 0.2651 → 0.2669 | 0.4061 → 0.4176 |
| 73 / 100 / wrap | 11 → 21 | 3 → 4 | 10.33 → 8.32 | 0.2893 → 0.3306 | 0.4662 → 0.6358 |
| 73 / 50 / deadly | 13 → 18 | 6 → 1 | 8.74 → 9.59 | 0.2682 → 0.2761 | 0.4527 → 0.5238 |
| 73 / 50 / wrap | 12 → 15 | 5 → 2 | 8.35 → 10.86 | 0.2850 → 0.2955 | 0.4477 → 0.4733 |
| 20260814 / 100 / deadly | 15 → 16 | 6 → 6 | 10.98 → 12.62 | 0.2535 → 0.2757 | 0.3966 → 0.4459 |
| 20260814 / 100 / wrap | 11 → 19 | 4 → 2 | 7.49 → 8.59 | 0.2759 → 0.2794 | 0.4661 → 0.4652 |
| 20260814 / 50 / deadly | 10 → 9 | 4 → 5 | 10.62 → 10.35 | 0.2659 → 0.2719 | 0.4102 → 0.6109 |
| 20260814 / 50 / wrap | 14 → 15 | 4 → 2 | 8.55 → 8.53 | 0.2725 → 0.2806 | 0.4738 → 0.5632 |
| 991 / 100 / deadly | 17 → 9 | 4 → 3 | 10.70 → 10.62 | 0.2617 → 0.2596 | 0.4050 → 0.4044 |
| 991 / 100 / wrap | 13 → 17 | 2 → 2 | 7.89 → 8.49 | 0.2777 → 0.2716 | 0.4420 → 0.4335 |
| 991 / 50 / deadly | 10 → 19 | 3 → 1 | 12.06 → 10.60 | 0.2716 → 0.2559 | 0.4323 → 0.5039 |
| 991 / 50 / wrap | 17 → 12 | 3 → 5 | 7.72 → 9.05 | 0.2757 → 0.2891 | 0.4334 → 0.4748 |

Exact kills **151 → 182 (+31)**; self deaths **50 → 39 (−11)**; wall deaths
**0 → 0**. Bigger/smaller/similar kills are **144 / 35 / 3**, with no ambiguous
masks. Deaths total **221**; opponent contacts account for **82.4%**. Hunting
kills **74 → 104** and staged kills **24 → 47** remain intention labels rather
than causal certificates. Natural runs still admit zero pockets.

Mean step **0.2718 → 0.2794 ms (+2.8%)**; final p99 range is
**0.4044–0.6358 ms**. Three p99s exceed 0.55 ms. Ten of twelve runs meet ≤5 self
deaths, versus nine previously; no run meets ≤8 oscillations/snake-min.
The broad behavioral/performance acceptance gates are not all met.

Three fresh alternating before/after runs of 20260814 / IQ100 / deadly on core 4
compare the preserved prior-review executable with this follow-up. Both replay
identical outcomes on repetition. No task compiler or benchmark runs concurrently.

| Pair / order | Prior review mean / p99 ms | Follow-up mean / p99 ms |
| --- | --- | --- |
| 1 / before–after | 0.2646 / 0.4177 | 0.2654 / 0.4105 |
| 2 / after–before | 0.2723 / 0.5606 | 0.2621 / 0.4087 |
| 3 / before–after | 0.2657 / 0.4353 | 0.2623 / 0.4097 |

Paired arithmetic means are **0.2675 → 0.2633 ms (-1.6%)**; median means
**0.2657 → 0.2623 ms (-1.3%)**. These same-session pairs show no
mean timing regression in this fixture. Timing still varies between sessions,
and these pairs do not establish a strict no-regression bound across all worlds;
the full matrix's +2.8% historical-session measurement and three failed p99
samples are retained above.

### Duels: prior review → follow-up

All eight scenarios, both IQs, identical jitters, aggression levels, mirrors and
role swaps: 1280 worlds per mode, with the same first-death stopping rule.
High/low means aggression 1.0/0.15; each value is kills per 40 worlds.

| Scenario / IQ | Responsive high / low | Limited high / low |
| --- | --- | --- |
| open_crossing / 100 | 0 → 0 / 0 → 0 | 27 → 29 / 18 → 18 |
| wall_escape / 100 | 0 → 0 / 0 → 0 | 0 → 0 / 0 → 0 |
| wrap_escape / 100 | 2 → 2 / 0 → 0 | 36 → 36 / 3 → 3 |
| equal_cluster / 100 | 0 → 0 / 0 → 0 | 1 → 1 / 1 → 1 |
| food_cutoff / 100 | 0 → 0 / 0 → 0 | 28 → 23 / 20 → 20 |
| long_encircle / 100 | 0 → 0 / 0 → 0 | 24 → 26 / 31 → 31 |
| feasible_crossing / 100 | 1 → 0 / 0 → 0 | 6 → 7 / 20 → 20 |
| infeasible_chase / 100 | 0 → 0 / 0 → 0 | 0 → 0 / 0 → 0 |
| open_crossing / 50 | 1 → 1 / 0 → 0 | 34 → 34 / 17 → 17 |
| wall_escape / 50 | 0 → 0 / 0 → 0 | 0 → 0 / 0 → 0 |
| wrap_escape / 50 | 0 → 0 / 0 → 0 | 32 → 33 / 4 → 4 |
| equal_cluster / 50 | 1 → 1 / 0 → 0 | 0 → 0 / 0 → 0 |
| food_cutoff / 50 | 0 → 0 / 0 → 0 | 24 → 16 / 32 → 32 |
| long_encircle / 50 | 0 → 0 / 0 → 0 | 34 → 34 / 32 → 32 |
| feasible_crossing / 50 | 1 → 0 / 0 → 0 | 15 → 15 / 12 → 12 |
| infeasible_chase / 50 | 0 → 0 / 0 → 0 | 0 → 0 / 0 → 0 |

Responsive kills **6 → 4 (−2)**; attacker deaths **2 → 1**; victim self/wall
**0 / 0**. Both previously responsive feasible-crossing kills disappear.
Remaining contacts are two IQ100 seam kills, one IQ50 open crossing and one IQ50
equal-cluster contact; the equal-cluster attacker also dies. Infeasible chase
kills remain zero. Limited kills **451 → 444 (−7)**; attacker deaths **30 → 31**;
victim self deaths **3 → 2**, wall deaths zero. These are not forced-kill claims.
The responsive attack-quality limitation remains unresolved.

### Final verification

`cargo test --frozen --offline` passes **164 tests**: 79 library, 81 private
allocation harness and four integration. Final CTest passes **14/14**, including
mechanics parity, recorded parity, C ABI, native simulation and QML integration.
The Release 6000-segment / 450-food test retains all segments and reports zero
allocations/reallocations for 1000 measured ticks plus 2000 reconfiguration ticks,
including the first AI ticks. Its isolated measurement is **0.5098 ms mean /
0.6487 ms p99**, versus the previous separate-session sample **0.5053 / 0.6622**;
a strict 0.50 ms cap remains unestablished. `git diff --check` passes.

Reproduction uses the commands in the verification section below. The exact
sequential final command script is `/tmp/ai4-rollout-fix/verify.sh`; alternating
normal comparisons use `/tmp/ai4-rollout-fix/pairs.py`. Raw final logs and immutable
executables are in `/tmp/ai4-rollout-fix`: `cargo.txt`, `cmake-configure.txt`,
`cmake-build.txt`, `ctest.txt`, `release-build.txt`, `allocation-release.txt`,
`scorecard-final.txt`, `duels-final.txt`, `limited-final.txt`,
`normal-before-[1-3].txt`, `normal-after-[1-3].txt`, `normal-pairs.txt`,
`regressions-before.txt`, `scorecard-final` and `competition-final`.
No commit, push, review loop or persistent process is created by this worker.

## Scope and accounting

The base is `d2e5eaf1bc5ec6abf82b248526cfa7c6cb5cdd24` plus the uncommitted
iteration-3 candidate and imported Rust/integration work. The original crate is
preserved in `/tmp/ai4-v3-original`; exact-accounting v3 is `/tmp/ai4-before`.
Before and after share the same mechanics event hook and fixture/observer code.
V3's added Rust-only debug fields return zero for unavailable attack phases;
they do not change its controls. No imported integration code is edited here.

`World::collision_events()` borrows a fixed `[CollisionEvent; MAX_SNAKES]`.
Each event captures completed tick, victim slot/generation, final reason,
collision-time lengths, head sweep, and owner generations/lengths before any
explosion. Body contacts retain mechanics' first owner; simultaneous lethal
head contacts retain a mask. Wall/head/body precedence and iteration order are
unchanged. Records expire at the next collision pass and snapshots copy them.
There is no event allocation, RNG draw, additional collision search, C export,
or exported structure layout change.

The primary metric counts deaths with an opponent contact. A victim is counted
once, including simultaneous contacts; ambiguous masks are reported separately.
Bigger/smaller means a collision-time length difference of at least four;
remaining single-owner contacts have similar lengths. Deaths of the contact
owner in the same pass remain visible: a kill does not imply attacker survival.
Self/wall losses are never silently credited to a nearby hunter. Natural food
capture attribution remains a nutrition-and-proximity observer estimate.

## Controller

- A single preallocated controller still chooses from **11 candidates**, with
  **72 ticks** of mandatory coverage and **138** on scheduled enclosure-risk
  decisions. Strategy/urgent quotas remain **2 + 2**. The existing spatial grid,
  4096-record safety cap, future-trail space check, conservative growth/tail
  release, turn hysteresis and two extended escape slots remain bounded.
- `attack.rs` replaces the two ordinary turn-then-straight slots during hunts
  with at most two cutoff finalists. It samples prey arrivals at **0.6, 1.0,
  1.4 seconds**, constructs both perpendicular crossing sides, and deposits
  the barrier about 0.2 seconds early. Approach bearing refinement is bounded
  to three discrete simulations. An arc to the approach bearing and straight
  burst lead into a slower crossing arc and straight exit. Rush is the hunter's
  aggression on approach and **0.15** in the crossing turn.
- Each stage uses `motion_limits()` and mechanics' angle clamp and 30 Hz
  movement law. Ordinary safety rollout validates the generated path. Six
  physical prey replies (straight, immediate/delayed left/right, escape burst)
  are evaluated only for the two safe finalists on strategy searches. When enabled,
  current and deposited self geometry reject invalid replies. Attacker barrier lifetime
  uses cumulative distance traveled, not conservative slow tail-release time.
  These are samples, not a forced-kill proof; forecasts hold current size and
  nutrition rather than replaying future food consumption.
- Retained attacks carry target generation, side, crossing, headings and stage
  deadlines. Side continuity and commitment compete with reply pressure in the
  existing safety ranking. Stale/dead targets, a passed crossing, lost size or
  speed advantage, expiry and recovery cancel an attack and its alternatives.
  Finalists are decision-local scratch. A shared predicate binds every retained
  attack/finalist to prey slot/generation and checks deadlines, recovery, size,
  speed and passed crossings before rush, rollout or reply evaluation. Every
  published tick still checks actual nearby geometry. A new safe attack can supersede an old
  one; visible retargeting remains a limitation.
- All rival forecasts now use **observed position, heading, recent actual turn
  and applied rush**. Observed curvature decays over 0.3 seconds into straight
  travel. They never read another snake's private desired heading, retained
  exit, goal or orbit. One forecast is shared across controllers' decisions.
- A fixed life trait derived from turn bias gives opponent-response revisions
  every **2–8 ticks**. Future rival-head/neck selection uses this cadence; the
  immediate swept check, current bodies, wall viability and self lookahead run
  every tick. Scheduled strategy quotas can further delay tactical goal updates.
  Tactical revisions stay pending until a strategy slot performs them; recovery
  early returns do not consume them. Forecast checks have a separate deadline
  because they are performed during rollout. Attack expiry/identity validation
  runs immediately each tick. IQ's existing physical turn-rate effect is unchanged.
  No random steering mistakes or environmental-safety reductions are added.
- `pocket.rs` admits an existing wall-side U only with sufficient body coverage,
  initial prey containment, body-distance reserve and checked continuation.
  Prey escape timing is not certified against all responses; successful
  offensive gap closure and a finishing mechanism remain unproven.
  Wrapping/seam pockets and open-space automatic coils are rejected. A legal
  inner pitch bridges the U before contraction. The explicit spiral tangent has
  bounded cross-track correction; radius follows angular progress, without the
  old `0.622R` inward equilibrium. For `k = pitch / 2π`, curvature is
  `(r² + 2k²)/(r² + k²)^(3/2)` and `vκ ≤ ω` is checked. Pitch lies strictly between
  `1.48rA + 2` and `2×0.78(rA+rB) − 2`. Actual traveled distance ages the barrier.
  Prey escape, release, reverse winding, curvature/floor or timeout aborts it.
  The positive wall-U test proves a checked entry, not a finished enclosure.
  Natural runs admit **zero** such pockets; no coil kills are claimed.

## Previous review natural scorecard

3440×1440, density/trails/scale/speed 100, 14 slots, self collision on, 30 Hz.
Each row is a complete eight-minute run. Final runs execute consecutively on
physical P-core **4**, without concurrent task compilers or diagnostic replays.
The left values are the README's pre-review v4 results; timings compare separate
sessions rather than alternating pairs. Every final p99 is below 0.55 ms.

| Seed / IQ / walls | Opponent kills v4 → fixed | Fixed bigger / smaller / similar | Self v4 → fixed | Oscillations/snake-min v4 → fixed | Avg ms v4 → fixed | p99 ms v4 → fixed |
| --- | ---: | --- | ---: | --- | --- | --- |
| 73 / 100 / deadly | 14 → 8 | 7 / 1 / 0 | 5 → 6 | 8.18 → 8.90 | 0.2623 → 0.2651 | 0.4328 → 0.4061 |
| 73 / 100 / wrap | 15 → 11 | 10 / 0 / 1 | 4 → 3 | 8.34 → 10.33 | 0.2868 → 0.2893 | 0.5021 → 0.4662 |
| 73 / 50 / deadly | 16 → 13 | 11 / 1 / 1 | 2 → 6 | 7.44 → 8.74 | 0.2570 → 0.2682 | 0.4377 → 0.4527 |
| 73 / 50 / wrap | 13 → 12 | 7 / 5 / 0 | 2 → 5 | 6.71 → 8.35 | 0.2740 → 0.2850 | 0.4526 → 0.4477 |
| 20260814 / 100 / deadly | 12 → 15 | 13 / 2 / 0 | 3 → 6 | 10.81 → 10.98 | 0.2641 → 0.2535 | 0.4470 → 0.3966 |
| 20260814 / 100 / wrap | 21 → 11 | 10 / 1 / 0 | 0 → 4 | 6.47 → 7.49 | 0.2701 → 0.2759 | 0.4555 → 0.4661 |
| 20260814 / 50 / deadly | 15 → 10 | 5 / 5 / 0 | 5 → 4 | 10.22 → 10.62 | 0.2705 → 0.2659 | 0.4710 → 0.4102 |
| 20260814 / 50 / wrap | 14 → 14 | 9 / 4 / 1 | 2 → 4 | 6.60 → 8.55 | 0.2791 → 0.2725 | 0.4631 → 0.4738 |
| 991 / 100 / deadly | 11 → 17 | 11 / 5 / 1 | 6 → 4 | 10.80 → 10.70 | 0.2671 → 0.2617 | 0.4533 → 0.4050 |
| 991 / 100 / wrap | 19 → 13 | 7 / 5 / 1 | 4 → 2 | 6.14 → 7.89 | 0.2655 → 0.2777 | 0.4464 → 0.4420 |
| 991 / 50 / deadly | 14 → 10 | 9 / 1 / 0 | 6 → 3 | 8.71 → 12.06 | 0.2525 → 0.2716 | 0.4306 → 0.4323 |
| 991 / 50 / wrap | 14 → 17 | 14 / 3 / 0 | 3 → 3 | 7.23 → 7.72 | 0.2741 → 0.2757 | 0.4663 → 0.4334 |

No ambiguous masks occurred. Bigger-owner kills are **132 → 113**,
smaller-owner kills **42 → 33**, and similar-length kills **4 → 5**. Deaths total
**220 → 201**; **75.1%** of final deaths have an exact opponent contact.
Mean kills/run are **14.83 → 12.58**.

Mean step is **0.2686 → 0.2718 ms (+1.2%)**, final range
**0.2535–0.2893 ms**, p99 **0.3966–0.4738 ms**. Self deaths are 2–6;
nine of twelve meet ≤5, versus ten before review. Oscillations are 7.49–12.06;
three meet ≤8, versus six before review. Wall deaths remain zero in every run.
**The required non-regression in self safety is not achieved.**

Kills with hunting intention change **83 → 74**; kills with a staged cutoff
active change **12 → 24**. These flags label controls at contact, not causal
proof. Hunting deaths are **8 → 5**, staged attack deaths **1 → 0**.
Hunting time is **13004.91 → 16300.86 snake-seconds**, rushing time
**7644.07 → 10044.51**. Food capture/min is **402.12 → 387.03**, mean length
**70.25 → 70.06**. No general enclosure kills are claimed.

## Previous review controlled fixtures

Eight scenarios, IQ 100/50, ten identical jitters crossed with aggression
1.0/0.15, mirror and role swap: **40 worlds per table cell**, 1280 per mode.
Normal duration is 20 seconds, stopping at the first death before respawns.
The infeasible chase lasts **3 seconds**, isolating the impossible attack window
rather than counting an unrelated encounter after a wall turnaround.

`--scripted-prey` now means competent **limited** prey: bounded heading choices,
72-tick wall/self checks, own deposited trail checks and immediate rival-body
checks. A passed food patch never asks it to reverse into its neck. It does not
anticipate opponents' next maneuvers. The curved fixture uses **130 actual
samples at radius 180**, meeting v3's length/turn eligibility at both IQs with
an open gap. V4 correctly rejects its unsupported open-space coil.

Kills use the exact record for **any uniquely attacker-owned opponent death**,
including third rivals in triads and contacts of any size. Food shares normalize
attacker/victim roles across swaps. The pre-review fixtures had no victim
self/wall deaths. Corrected responsive fixtures still have none; corrected limited fixtures have three victim self
deaths and zero victim wall deaths.

| Scenario / IQ | Responsive high kills v4 → fixed | Responsive low kills v4 → fixed | Limited high kills v4 → fixed | Limited low kills v4 → fixed | Responsive high attacker deaths v4 → fixed | Limited high attacker deaths v4 → fixed |
| --- | --- | --- | --- | --- | --- | --- |
| open_crossing / 100 | 0 → 0 | 0 → 0 | 35 → 27 | 40 → 18 | 0 → 0 | 14 → 0 |
| wall_escape / 100 | 0 → 0 | 0 → 0 | 0 → 0 | 0 → 0 | 0 → 0 | 0 → 0 |
| wrap_escape / 100 | 8 → 2 | 0 → 0 | 40 → 36 | 3 → 3 | 0 → 0 | 0 → 6 |
| equal_cluster / 100 | 1 → 0 | 0 → 0 | 0 → 1 | 0 → 1 | 0 → 0 | 0 → 0 |
| food_cutoff / 100 | 0 → 0 | 0 → 0 | 17 → 28 | 33 → 20 | 0 → 0 | 0 → 0 |
| long_encircle / 100 | 0 → 0 | 0 → 0 | 27 → 24 | 32 → 31 | 0 → 0 | 0 → 0 |
| feasible_crossing / 100 | 0 → 1 | 0 → 0 | 2 → 6 | 0 → 20 | 0 → 0 | 0 → 0 |
| infeasible_chase / 100 | 0 → 0 | 0 → 0 | 0 → 0 | 0 → 0 | 0 → 0 | 0 → 0 |
| open_crossing / 50 | 1 → 1 | 0 → 0 | 39 → 34 | 40 → 17 | 0 → 0 | 22 → 12 |
| wall_escape / 50 | 0 → 0 | 0 → 0 | 0 → 0 | 0 → 0 | 0 → 0 | 0 → 0 |
| wrap_escape / 50 | 4 → 0 | 0 → 0 | 36 → 32 | 5 → 4 | 0 → 0 | 2 → 0 |
| equal_cluster / 50 | 0 → 1 | 0 → 0 | 0 → 0 | 0 → 0 | 0 → 1 | 0 → 0 |
| food_cutoff / 50 | 1 → 0 | 0 → 0 | 34 → 24 | 36 → 32 | 0 → 0 | 1 → 0 |
| long_encircle / 50 | 2 → 0 | 0 → 0 | 36 → 34 | 32 → 32 | 2 → 0 | 2 → 0 |
| feasible_crossing / 50 | 2 → 1 | 0 → 0 | 12 → 15 | 16 → 12 | 0 → 1 | 1 → 0 |
| infeasible_chase / 50 | 0 → 0 | 0 → 0 | 0 → 0 | 0 → 0 | 0 → 0 | 0 → 0 |

All responsive cells: kills **19 → 6**, attacker deaths **2 → 2**.

All limited cells: kills **515 → 451**, attacker deaths **100 → 30**.

Responsive feasible crossing is now **1/40 at each IQ**; the IQ50 kill also
kills the attacker. Infeasible chases remain zero. Responsive aggressive seam
kills are **2/40 at IQ100, 0/40 at IQ50**, with no attacker deaths there.
The IQ100 crossing kill takes 9.800 seconds; seam kills average 3.550 seconds.
The remaining responsive IQ50 contacts are open crossing and equal cluster;
the equal-cluster contact also kills the attacker. Responsive mode has no victim
self/wall deaths. Limited mode has three victim self deaths (IQ100 wrap escape) and zero victim wall deaths; these deaths
are not credited as kills. Limited prey remains easier to contact and does not establish forced kills against responsive prey.

## Previous review mature cost and allocations

The cap fixture starts and ends with **6000 segments / 450 food** for 1000
measured ticks, then tests 2000 ticks after deadly/self reconfiguration. Both
phases allocate/reallocate **zero** times, including first AI ticks.

The final isolated cap check measures **0.5053 ms avg / 0.6622 ms p99**,
with zero allocations/reallocations and all 6000 segments retained. This single
sample exceeds 0.50 ms; the historical timing pairs below are not measurements
of the corrected controller.

Three fresh alternating P-core-14 pairs compare the preserved pre-review v4
controller with the final corrected controller. All six cap/reconfiguration
checks allocate zero times; no concurrent task compiler or replay runs.

| Pair | Pre-review v4 avg / p99 ms | Fixed avg / p99 ms |
| --- | --- | --- |
| 1 | 0.5336 / 0.7164 | 0.5286 / 0.7128 |
| 2 | 0.5203 / 0.6925 | 0.6110 / 3.5665 |
| 3 | 0.6177 / 0.9007 | 0.5223 / 0.6830 |

Median average is **0.5336 → 0.5286 ms (−0.9%)**. The corrected pair-2 slowdown
and p99 spike are retained in the table. Timing varies in both executables;
these runs do not establish a strict 0.50 ms cap for either controller.

Historical pre-review timing: three alternating same-core pairs on P-core 14:

| Pair | v3 avg / p99 ms | v4 avg / p99 ms |
| --- | --- | --- |
| 1 | 0.4951 / 0.5665 | 0.4725 / 0.5662 |
| 2 | 0.6968 / 1.1014 | 0.6629 / 0.9348 |
| 3 | 0.4898 / 0.5674 | 0.4755 / 0.5590 |

Median average **0.4951 → 0.4755 ms (−3.96%)** meets 0.50 ms. Pair 2 shows a
shared slowdown in both algorithms and is disclosed, not discarded. Two earlier
core-6 after samples measured 0.5353/0.5187 ms versus 0.4941 ms before; this timing
variance prompted the three paired repeats. A strict every-sample 0.50 ms ceiling
is not established on this shared machine. Normal scorecard limits are met.

## Historical pre-review paths and exact events

These describe the pre-review v4 executable, not the corrected controller.
They are log-based descriptions, not a blinded visual judgment:

- **Natural 20260814/IQ100/deadly, tick 4367 (145.57s):** snake 1 selects snake 5
  around 143s and rushes diagonally toward its predicted route. The victim first
  turns toward open rightward travel, then uses 0.25 escape rush and turns down.
  The hunter briefly selects side +1 cutoff bursts, slows to 0.15 during the
  final crossing turn, and wins a head contact at (2077,349), lengths **62/50**.
  Several plans are superseded; it reads as pursuit plus an escape and closing
  contact, not a perfectly held three-stage maneuver.
- **Responsive seam, IQ100, jitter 1, mirrored, original roles, tick 112:** the
  attacker bursts around the victim's changing turns from 1.8s, crosses the
  seam ahead at 3.2s, then bends back across its rightward exit. The victim tries
  a late nearly-horizontal turn at 3.6s; at 3.73s the **72/24** head contest
  resolves for the attacker. Both geometry and the contact survive wrapping.
- **Responsive feasible crossing, IQ50, jitter 0, original roles, tick 421:**
  the hunter approaches from the victim's upper-left around 11.8s. The victim
  turns repeatedly to escape; the hunter arcs around its outside, bursts along
  the same side from 12.6–13.6s, then crosses the victim's leftward exit at 14.03s
  and wins **72/41**. Earlier side switches make this less clean than the desired
  stable attack. No wall/self death supplies the kill.
- **Natural tick 5154 (171.8s):** the **80/67** body contact is exact, but the
  owner is disengaging without hunt/cutoff flags. This is an incidental moving
  barrier encounter; it is not labeled a deliberate trap.

## Rejected pilots and remaining decision

Staged attacks with privileged forecasts: reference opponent kills 9→10 but
self deaths 3→7. Observation-only prediction then reached 15 kills / 2 self;
the restricted-pocket/trait pilot reached 12 / 3. A steering deadband and earlier
extension gate lost responsive seam kills; endpoint/burst-duration refinement
reached only 8 kills / 7 self. These were not retained. A separate hard-commit
pilot in `/tmp/ai4-commit-pilot` raised the reference to 18 kills / 1 self but
eliminated the feasible responsive crossing kills; it was also not retained.
The pre-review full matrix was authoritative for that candidate. The corrected
controller is measured separately in the tables above; the self-safety gate
remains failed.

The remaining problem is robust, stable attacks against responsive prey,
especially IQ100, plus the failed self-safety gate and excess oscillation.
No gameplay rules were changed. If the next experiment needs a rule change,
prototype **a 0.8s full burst costing two segments with a 1.2s recovery interval**,
available to both sides. Require the attacker to retain its four-segment winning
margin after the cost. That creates observable spent-escape windows; test it
separately before changing the length-speed penalty. This is a recommendation,
not evidence that current mechanics make stronger AI impossible. Preserve the
historical mechanics oracle and explicitly version any such rule experiment.

## Verification and reproduction

Distribution Rust only, frozen/offline, no dependency or version changes:

```sh
RUSTC=/usr/bin/rustc /usr/bin/cargo test --frozen --offline --manifest-path rust/snakes-core/Cargo.toml
RUSTC=/usr/bin/rustc /usr/bin/cargo build --release --frozen --offline --manifest-path rust/snakes-core/Cargo.toml --example ai_scorecard --example ai_competition
cmake -S . -B build-ai4 -G Ninja -DCMAKE_BUILD_TYPE=Release -DBUILD_TESTING=ON
cmake --build build-ai4 -j 6
ctest --test-dir build-ai4 --output-on-failure
RUSTC=/usr/bin/rustc taskset -c 14 /usr/bin/cargo test --release --frozen --offline --manifest-path rust/snakes-core/Cargo.toml --test allocation zero_allocations_with_ai_at_caps_and_after_reconfiguration -- --nocapture
taskset -c 4 rust/snakes-core/target/release/examples/ai_scorecard 8 20260814 100 deadly --ai-only
taskset -c 18 rust/snakes-core/target/release/examples/ai_competition
taskset -c 18 rust/snakes-core/target/release/examples/ai_competition --scripted-prey
taskset -c 18 rust/snakes-core/target/release/examples/ai_competition --scenario feasible_crossing --trace
cc -std=c11 -Wall -Wextra -Werror -I rust/snakes-core/include rust/snakes-core/tests/ai_debug_smoke.c build-ai4/cargo-target/Release/release/libsnakes_core.a -ldl -lpthread -lm -o /tmp/ai4-debug-smoke
/tmp/ai4-debug-smoke
```

Cargo passes **164 tests** (79 library, 81 private allocation harness, four
integration); full CTest **14/14**, including parity, recorded parity, ABI and
native/QML integration. The C smoke client builds with warnings as errors and
runs. `git diff --check` passes. Regressions cover exact event precedence/masks,
pre-explosion lengths/generations, expiry/snapshot, stage movement/continuation,
unreachable cutoffs, independence from private plans, immediate environmental
checks under delayed response, spiral radius/curvature/distance aging and a
positive wall-U entry. Existing observer determinism and mechanics goldens pass.

Raw logs, immutable executables and scripts are in `/tmp/ai4-results`:
`paired-before-*` / `paired-after-*`, `duels-before-final.txt` /
`duels-after-final.txt`, `limited-before-final.txt` / `limited-after-final.txt`,
`stress-final-before-[1-3].txt` / `stress-final-after-[1-3].txt`,
`readability-natural.txt`, `cargo-final.txt`, `ctest-final.txt` and
`cmake-build-final.txt`. `/tmp/ai4-paired.py` runs the 12 alternating pairs.
No commit, push, review loop or persistent process is created by this worker.
The review-fix loop remains the orchestrator’s responsibility.

## Review classes and sibling audit

- **Schedules/deadlines:** scheduled and urgent strategy entry, escape/orbit early
  returns, forecast cadence, and attack/hunt/stage/recovery deadlines were checked.
  Tactical response deadlines now advance only after tactics and finalist
  generation run. Forecast revisions keep a separate performed-work cadence;
  attack validity is checked each tick. Other timers are absolute tick comparisons,
  and food progress advances only on observed progress; they do not discard pending
  work. Recovery pauses can defer tactics but cannot permanently phase-starve it.
- **Retained state:** active attacks, both alternatives, their prey identities,
  heading/exit/turn/commitment/rush controls, and goal tracking were checked at hunt
  expiry/lost advantage, dead/replaced prey, threat dodge, orbit/area recovery,
  strategy early returns and pocket entry. All attack cancellations share
  `clear_attacks`; finalists are decision-local. All attack consumers use
  `attack_usable`. Dead/replaced prey also clears stale goal tracking.
- **Geometry certification:** winning-head suppression, pursuit straightening
  after forecast defeat, future necks, current bodies, own deposited trails and
  response head scoring were checked. `head_contact` uses matching single-tick
  sweeps in rollout and prey replies; neck safety runs before forecast defeat
  suppresses later rival checks. Losing-head/neck/self aggregate sweeps remain
  conservative rejection tests, never lethal-contact certificates. Current bodies
  are never suppressed by a forecast defeat.
- **World settings:** both current-own-body and deposited-own-trail reply tests,
  both corresponding limited-prey benchmark tests, rollout body/self/area masks,
  wall checks, and wrapping queries were checked. The four simulated-response
  checks now read `self_collisions`; wall checks already read `deadly_walls`, and
  geometry uses world displacement/canonicalization. Pocket pitch reserves are
  intentionally conservative geometry constraints, not simulated self deaths.

Six added regressions cover the four classes plus aligned/seam head contacts and
shared attack identity/advantage/deadline checks. The four primary class tests
fail on the original implementation in an isolated `/tmp` copy and pass after
fixing; the copy adapts the newly added private Attack identity/cadence fields
for compilation and omits the two helper-specific tests. The four primary
behavioral assertions are unchanged. No mechanics, exported functions or
`repr(C)` layouts changed.

The first shared-response pilot measured 139 kills / 52 self deaths; separating
performed forecast work from pending tactics measured 162 / 50; the final
shared-control cancellation version measured 151 / 50. All have zero wall deaths.
No unrelated behavioral tuning was retained to hide the safety regression.

Final logs and immutable executables are in `/tmp/ai4-review-fixes`:
`scorecard-final.txt`, `duels-final.txt`, `limited-final.txt`, `cargo.txt`,
`ctest.txt`, `allocation-release.txt`, `cap-before-[1-3].txt`,
`cap-after-[1-3].txt`, and `regressions-before.txt`.

### R1 V2 mechanics groundwork

`Config.rules` selects `RuleSet::Classic` (Rust's compatibility default) or
`RuleSet::V2`. The C ABI defaults to V2 (`rule_set = 0`); `1` explicitly selects
Classic, and `2` explicitly selects V2. Parity replay and JS goldens select
Classic. The scorecard and mechanics benchmark select V2 unless `--classic` is
passed. Changing rules restarts the world and invalidates the AI geometry epoch.

V2 maps the old AI rush requests of at least 0.5 to a fixed 0.6 boost request
when `World::boost_ready` is true. Motion observations include an ongoing burst,
including when the current request is zero. The old dodge and crossing rushes
are zero in V2. Burst cost budgeting and future-duration rollouts remain R1 step
B work. `Controller::intent_flags` publishes HUNTING/TRAPPED after steering;
its default `None` preserves explicit `World::set_intent_flags` hooks. AI flags
use prey/coil presence and the existing finalist reachable-area result (below
1.5 times its turnaround-area requirement, only when uncapped).

ABI v2 keeps the snapshot export and adds `snakes_core_export_extras` for the
latest tick's bounded events and the reserved empty item array. A multi-tick
step exports only the final tick's events. Event reads are repeatable. Corpse
segments are part of frame sizes, but never statistics or live collision/AI
geometry. The current renderer ignores the new state and skips corpse records.

## Validated control retention

Every rollout records its own turn deadline and exit heading alongside its
initial desired heading, goal-tracking mode, attack stages and rush. One commit
helper installs those controls after clearing old tactics; diagnostic snapshots
read the same candidate fields. Scratch-slot aliases and boosted continuations
therefore retain the maneuver whose path was checked. Narrow-space escape
bookkeeping applies only to nontracking, nonattack fallbacks, since entering
escape mode invalidates the goal, pocket and attack dependencies of a rollout.

## R3 power-ups

Food and capsules share target scoring, rival arrival estimates, persistence,
routing and the checked rollout. Magnet adds temporary scavenging utility only
when the food can be reached before expiry. Its five finalists use the ordinary
turnaround-space query to discount enclosed death fields. Food already inside
Magnet's pull radius needs no steering detour. These checks use the existing
fixed shortlist and shared spatial storage.

Surge's cutoff admission and forecasts use free burst pricing. Each attack
records whether its initial burst was free, so later validation retains the
original payment schedule even after the effect ends. Candidate scoring uses
the same world-owned burst cost. Phased rivals provide no immediate prey bonus.

Phase removes body/head/self obstacles only for the corresponding forecast
steps. The rollout checks both sides of expiry and extends the horizon through
expiry plus eighteen tangible steps, bounded by the existing 138-step limit.
Walls remain physical. No effect changes scripted controls or Classic rules.

## Forecast counter and query invariants

The world decrements effect counters before steering. The shared AI effect
clock uses step one for this movement, and step `ticks + 1` for the first
expired movement. `effects::forecast_motion` and cached
`Motion` schedules share that convention, latch burst price when it starts,
and retain physical motion through effect and boost expiry. The public World
forecast API accepts pre-tick counters instead; do not use it inside steering.
Cooldown advancement and Frost cancellation follow the same mechanics order.

Phase contact lethality requires both snakes to be tangible at the contact
step. This applies to cutoff replies (including victim self contacts), rollout
heads/current bodies/deposited bodies/self trails, pocket admission/retention,
food competition's head-size advantage, and pursuit pressure/barrier valuation.
Forecast sweeps split at every snake's Phase expiry and first tangible movement.
Phase does not prevent eating food, racing for food, or hitting walls.

`spatial::query_radius` derives broad bounds from the narrow contact threshold,
safety margin, effect reserve, and motion. Bucket body queries use maximum body
widths and margins, include Surge's scaled margin, and cover observed first-step
motion. Future deposited edges include both swept paths' traveled distances.
Magnet food reach includes the particle radius; food ring discovery uses unique
buckets with an explicit search budget rather than a contact-radius cutoff.

## R3 aggression after effect/contact correctness fixes

Keep the shared post-decrement effect schedules, both-party Phase contact
checks, and broad-phase bounds covering every narrow-phase threshold. Raise the
reward for a physically blocked sampled reply from 100 to 150 progress units;
reply deaths from self/walls and intangible contacts still earn no attack reward.
Retained cutoffs now account for their planned Surge speed expiry without
repricing a free burst or accepting an unplanned motion change. Storage remains
preallocated; the existing simultaneous-effect and cap allocation gates pass.

An immutable, CPU-8-pinned, eight-minute 12-config comparison repeats in
before/after then after/before order. Exact kills change **141 → 162**, self
deaths **22 → 17**, and wall deaths remain **0**. All outcomes reproduce in both
repeats. Equal-weight mean tick changes **0.246250 → 0.253738 ms (+0.007488 ms)**,
inside the requested +0.01 ms gate. Shared-host timing varies: the two individual
paired differences are +0.011987 and +0.002990 ms. No stronger CPU bound is claimed.

Fresh release build, **744 Cargo test executions**, and **14/14 CTest tests**
in `build-fix` pass, including Classic and recorded parity. A duel-accounting
regression now forces an unrelated body death on tick one, so its continuation
assertion does not depend on the former AI's exact tick-38 trajectory; the
ordinary benchmark scenarios remain unchanged.

The legacy public pre-tick forecast APIs remain a scope issue in
`world/query.rs`. A test-only pre-tick adapter uses the shared single-burst
state calculation and passes the effect/Phase expiry, payment and cooldown
matrix. Routing the public APIs through it is prepared and verified separately,
but is not applied within the current allowlist. Optional `r3_scorecard`
`--attack-diagnostics` and `--death-diagnostics` runs are separate from timing
runs because they compute extra candidates and replay alternatives.

## Post-movement capture and replacement classes

Capsule replacement is forecast at pickup, before the same movement's collision
checks. The shared all-snake three-item forecast handles incidental capsules, expiry,
refresh/replacement order, and one collection per item. Its Phase result governs
current bodies, rival heads, deposited necks, self trails, pursuit valuation and
endpoint occupancy. Cutoff replies include every live snake's pickups and
resolve shared capsules in World snake-ID order. Pickup transitions split the
batched sweep; a newly tangible sweep starts at the preceding movement point.
Regression siblings cover Surge/Magnet replacement versus Phase refresh, wrap
seams, all five positions around a four-step sweep, losing heads, self contacts,
item lifetimes, multiple capsules and reply ownership.

Pending food/capsule overlap remains a pursuit until a post-movement capture or
an existing food ownership claim. Steering can finish early when its proposed
straight movement still captures the target, preserving safe Magnet passes.
Selection, retained control, rollout tracking and capture scoring follow that
rule. Regressions run lateral boundary overlaps through World::step for Spark,
Shard/death fields, Pellet, Prism and capsules, with and without Magnet; ownership,
Magnet expiry and routing waypoints remain independent checks. Typed contact
reach and arrival-to-capture valuation from the preceding fixes remain intact.

Both ecosystem scorecards count Phase use only for otherwise-lethal contacts.
A head encounter won by at least four segments contributes no use, while rival
body and enabled self contacts remain independent. Example regressions compare
winning, tied and losing head encounters with World outcomes and preserve the
independent body/self checks. Attack diagnostics report rejected viable attacks
by physical blocked-reply count and their score deficit; combat attribution
continues to use exact World collision events.


Near-term attack opportunities use V2 arrival windows at 12/18/24 ticks instead
of 18/24/36. The number of refinements/finalists, exact blocked-reply reward
(150), physical safety gates and Classic windows remain unchanged. The
`v2_near_term_cutoff_is_safe_and_produces_an_exact_opponent_kill` regression
covers mirrored wall/wrap crossings, checks the full 72-step safety horizon,
and verifies an actual World body kill with exactly the attacker's owner bit.
Rejected-attack diagnostics motivated the timing experiment: 2,926 viable
positive-reply candidates lost ranking with a mean deficit of 150.886 units;
raising that reward alone failed the combined outcome gates.

Acquiring Phase must also preserve prior corporeal checks. The fixed pickup
forecast requests individual sweeps while approaching a Phase capsule, using
the same already-computed distance and a four-step maximum-motion reserve.
`phase_acquisition_does_not_hide_the_preceding_corporeal_sweep` verifies an
actual second-step body death before a third-step Phase pickup. Body-cache
entries include candidate tangibility; the cache regression alternates tangible
and intangible queries at identical geometry to prevent unsafe reuse.


## Shared all-snake effect forecast

`forecast::{Forecast, Timeline}` is the single AI effect clock. It starts from
steering's post-decrement observation for every live snake. At each movement it
uses `before` for motion and food capture, resolves all endpoint capsule contacts
in World item order with lowest-live-snake-ID ownership, applies replacement and
Surge's unpaid-burst forgiveness, then exposes `at`/Phase masks for collisions.
Capsule lifetime, refresh, consumption and expiry share this clock. No future
spawns, future food growth or private rival controls are assumed. Fixed three-item
transitions and masks use controller-owned storage; no rollout allocates.
Conservative per-item travel bounds prune unreachable owners once. Candidate
rivals reuse observed paths until ownership changes their movement; only then
is that rival copied into the reusable scratch storage.
`Items` builds nominal/boosted reach bounds, prefix owner masks and nominal
endpoint contest masks once per observation. Each candidate tests only its own
endpoints and rivals whose movement diverges. If the candidate cannot reach a
capsule within its horizon, it reuses the shared timeline, rival paths and
motion limits directly; ordinary candidates use nominal reach bounds. Shared
preparation integrates one independent snake track at a time, then rebuilds
only rows with movement-changing pickups. Candidate records
retain compact pickup events rather than full per-step effect mask arrays.
Motion/radius reconstruction after Surge is cached by movement state and the
first unpaid-burst forgiveness event; Phase/Magnet alone do not rebuild motion.

Phase approach guards use cached upcoming endpoint hits for unchanged rivals,
and radial motion reserves for changed participants. Merely passing near a
Phase capsule does not force one-tick sweeps of every candidate. Known Phase,
Surge and movement-changing pickup events finish the prior sweep before
changing its geometry. Rival sweep curvature and deposited-neck exemptions
use actual forecast motion and traveled distance; possible Surge speed widens
only the broad-phase search bound.

Physical simulation and opportunity valuation share the observation/expiry
primitives but answer different questions. Strategy's `opportunities` clock
includes observed effects and unavoidable movement-one pickups under all
steering/boost options and lower-ID contests. A merely possible later rival
pickup must not erase a chase, food advantage or free-burst opportunity.
Collision rollouts still resolve actual endpoint pickups exactly. Checked free
Surge attacks value blocked replies more highly while retaining the same
collision, horizon and room gates. Old-neck tangibility uses actual traveled
distance, including burst/effect expiry, rather than future maximum speed.

Call sites moved to the shared forecast:

- `prepare`/`advance_rivals`/`update_envelopes`: synchronized observed rival paths,
  motion after pickups, Surge reach and Phase transitions.
- `rollout_simulation` (all thirteen candidate kinds, retained attacks, compound
  escape/boost and pocket tracking): own and rival pickups, both-party head and
  current/deposited body contacts, self trails, motion boundaries and safe scoring.
- `body_blocked_phase`/`cached_body_blocked_phase`: rival tangibility and Surge
  bounds/margins; cache keys include all snakes' Phase and Surge masks.
- `phase_mask`/`candidate_mask`: strategy/routing space and endpoint area queries.
- `cutoffs`: each refinement simulates all snakes' item ownership and movement;
  `reply_blocked`/`reply_simulation` include every live snake in each sampled response, including
  third-party capsule ownership, victim self contacts and attacker barriers.
- `pocket`/`pocket_usable`, `tactics` and retained-prey validation: both-party Phase
  contacts, threat/escape reach and prey eligibility.
- `target_arrival`/`target_score`/`food_race`, target shortlist/routing/retained
  steering and rollout capture/progress: shared effect tracks for Magnet reach,
  food/capsule competition, head-size advantage and scavenging room discounts.
- `Motion::forecast`, Phase horizon and observed Surge policy use the common
  observation/expiry primitives; retained attack limits use the same effect clock.

The seeded World-step oracle checks 96 worlds × four live IDs × 138 movements
(**52,992 snake-step observations**), including curved movement, wrap seams,
effect and item expiry, ordered replacements, free/paid boosts and Surge pickup
forgiveness. Separate World-backed regressions cover rival Phase replacement at
head/body contacts in both owner orders and geometries, all five sweep-boundary positions, three-way
ownership, third-party reply pickup, cache reuse across rival replacement and
rebuilding rival motion when a candidate changes capsule ownership. The primary
rival regression fails against the immutable pre-fix source snapshot.
Cached versus uncached alternative-control tests add **119,232 effect
comparisons** across both geometries, candidate IDs, boost modes and ordered
capsule replacements. Opportunity-clock regressions distinguish unavoidable
pickups from steering/contest possibilities. Shared-path equivalence tests
cover curved long snakes across Surge and burst expiry, where reduced turn
limits must clamp the previously observed curvature in both builders.

The pre-forecast input reproduces **161/19/0** kills/self/wall deaths and
**12/74 intended Surge uses (16.22%)**. Its simulation source and binary are
preserved under `/tmp/snakes-class-forecast/before`.

The last complete worker verification passed **908 Cargo test executions**,
including all allocation checks and the World oracle, plus a fresh native
build and **14/14 CTests** (133.97 seconds). CPU-8 alternating paired means
were **0.299335 → 0.280459 ms (−0.018876 ms)**, meeting the +0.01 ms gate.
Combat passed at **167/19/0** kills/self/wall deaths. Intended Surge use remained
below acceptance: **8/68 uses (11.76%)**, with no chained episodes. That frozen
source and its results are archived under
`/tmp/snakes-class-forecast/worker-event-gate`.

Shared capsule winner/timing metadata now gates rival movement updates. Matching
contests reuse the prepared paths and limits; a changed outcome marks only
movement-affected owners. A separate opportunity path cache keeps speculative
Surge acceleration out of prey admission, leading, cutoff targets and barrier
valuation. Physical collision/reply simulations retain the exact effect clock.

The current additional change gives observed free Surge cutoffs the same
three-window budget at 18/30/42 movements, rather than paid bursts' 12/18/24.
It preserves exact motion, collision and room validation. All **18 forecast
tests** and **8 Surge policy tests** pass. In the focused 20260814/IQ50/wrap
case, intended uses rose from 1 to 3 and one chained episode returned, while
self deaths rose from 0 to 2; this is not aggregate acceptance evidence.
The focused first-decision/pickup/expiry/cutoff allocation check also passes.
The current full matrix timed out after 30 minutes waiting for a `heavy` slot
(exit 124; no matrix execution). Aggregate behavior and performance of this
final policy change remain **unverified**.

Or run the complete verification directly:

```sh
/home/mjstanton/.local/bin/heavy python3 /tmp/snakes-class-forecast/verify-worker.py
```

The complete script runs frozen/offline release Cargo tests, the release
scorecard build, `cmake --build build-r3 -j 4`, full CTest, CPU-8 one-minute
stage/call-site profiles, and the original alternating eight-minute 12-config
paired protocol. Profile stages are prepare/strategy/rollout/final-area/total;
forecast call sites are reach bounds/shared paths/contests/candidate setup/
participant updates. Per-case diagnostic runs accept `--case=seed,iq,walls`.

## Capsule pursuit — 2026-10-03 worker results

This change starts from v0.10.0 (`97ab295f70d74c12be29308759f275bad1ac9d9b`).
Capsules now compete as strategic objectives with a base premium of 110,
effect-specific context, remaining-life urgency, turn-aware arrival estimates,
and stronger commitment than food. A useful active effect discounts replacement
until near expiry. Capsules behind the head can merit a deliberate safe turn;
ordinary rollout, collision, room and Phase-expiry checks still decide steering.
Clearly lost races are discounted to zero, rather than enticing suicidal grabs.

Close races admit a forward, turnable boost using the time saved by one actual
0.8-second burst. Larger contenders can propose checked cutoffs against rivals
approaching the same capsule; smaller contenders retain the direct route.
Changing to a capsule clears an unrelated hunt's retained steering immediately.
The capsule route remains an alternative to the cutoff.

Observed Surge extends prey search to 850 pixels and admits free forward bursts
only toward real prey, outside a conservative turning reserve. Each boosted
candidate still competes against ordinary controls under physical safety checks.
Active Magnet increases food-density preference and avoids starting unrelated
hunts. Phase valuation prefers trapped states and nearby larger rivals; its
existing otherwise-lethal crossing and first-tangible-sweep safety logic remains.
Activation is immediate, with no inventory: preserving an effect means avoiding
premature replacement during its current window, not delaying activation.

Production uses existing fixed scratch/grid/rival rows. No mechanics, spawning,
forecast code, dependencies, C ABI, renderer or C++ files changed.

### Measurement definitions

The scorecard's new observer runs outside `World::step` timing. Every capsule
record includes spawn tick/ID/kind, nearest live snake's unobstructed turn-aware
ETA at spawn, whether any actual retained target selected it, distinct targeter
count, maximum simultaneous contenders, contest ticks, picked owner/expired/
pending outcome, and pickup latency. `--capsule-trace` prints those records.
The ETA is an estimate, not an obstacle-aware guaranteed route. Target selection
is sampled at actual control calls so deaths do not erase contested objectives.
Pickup events must all resolve to spawn records. Pending capsules at the end of
eight minutes count against the pickup fraction; they are not called expiries.

Intended use is counted once per pickup episode (generation guarded):

- Surge: an actual checked staged attack, or actual boosted movement toward
  the controller's current prey. `surge_staged` is the stricter staged subset;
  `surge_chained` counts episodes with at least two new prey-directed/staged
  bursts. These subsets overlap and must not be added together.
- Magnet: an extra-reach stationary Spark claim with a current or previous
  food objective, or consumption of prism/death-field food while active.
  Extra reach compares the post-movement head to the original stationary food
  position, before pulling it inward. `magnet_extended` separately reports
  passive extra-reach benefit and is not itself evidence of intended use.
- Phase: an otherwise-lethal body/self/head contact while intangible, using
  the existing World-backed contact helper. Alive-at-expiry is reported
  separately; it does not count as intended use.

The baseline copy contains unchanged v0.10.0 policy plus only the read-only
selected-target accessor and identical scorecard instrumentation. Its measured
217 total pickups include 72 Surge pickups: the brief's 72 figure describes
Surge alone, not all capsules. This discrepancy is retained rather than treating
the baseline as a field where most capsules expired.

### Natural-run results

Twelve configurations, eight simulated minutes each: seeds 73/20260814/991,
IQ 100/50, deadly/wrap boundaries, V2, all three implemented effects enabled,
3440×1440, density/trails 100, self collisions enabled. Behavior counts below
are for one complete matrix, not the sum of repeated deterministic runs.

| Metric | v0.10.0 | Pursuit policy | Acceptance |
|---|---:|---:|---:|
| Picked / spawned | 217 / 257 (84.44%) | 223 / 253 (88.14%) | ≥75% |
| Expired / pending | 33 / 7 | 26 / 4 | Reported |
| Targeted capsules | 255 | 253 (all) | Reported |
| Contested capsules | 177 | 217 | Reported |
| Median pickup time | 6.300 s | 4.767 s | Reported |
| Median nearest ETA at spawn | 3.756 s | 3.868 s | Reported |
| Intended Surge / pickups | 20 / 72 (27.78%) | 54 / 77 (70.13%) | ≥40% |
| Staged Surge subset | 20 | 14 | Reported separately |
| Chained prey-burst Surge subset | 8 | 49 | Reported separately |
| Intended Magnet / pickups | 68 / 77 (88.31%) | 80 / 83 (96.39%) | ≥40% |
| Passive Magnet extra-reach episodes | 75 | 82 | Reported separately |
| Intended Phase / pickups | 16 / 68 (23.53%) | 32 / 63 (50.79%) | ≥40% |
| Alive at observed Phase expiry | 66 / 66 | 62 / 62 | Reported separately |
| Opponent kills | 170 | 167 | ≥165 |
| Self / wall deaths | 20 / 0 | 20 / 0 | ≤20 / 0 |
| Ambiguous attributed kills | 0 | 0 | Reported |

Pickup latency falls 24.3%; expiries fall from 12.84% to 10.28% of spawns.
The increase in Surge intended use is chiefly repeated prey-directed bursts,
not an increase in staged attacks. The aggregate combat count is three kills
lower and still meets the requested gate. Finite seeded runs are evidence for
these fixtures, not proof of universally optimal behavior or no future deaths.

Paired release measurements ran sequentially on CPU 8 in before/after/after/
before order, with identical instrumentation and no overlapping task benchmarks
or compilers. Each cell below averages the twelve case means (equal tick counts).

| Pair/order | Before mean ms | After mean ms | Delta ms |
|---|---:|---:|---:|
| 1: before → after | 0.378150 | 0.333850 | −0.044300 |
| 2: after → before | 0.346775 | 0.361492 | +0.014717 |
| Both pairs, equal weight | 0.362463 | 0.347671 | −0.014792 |

The combined paired mean meets the +0.01 ms budget. The reversed pair alone
exceeds it by 0.004717 ms, so shared-machine variability is material; this is
not evidence of a repeatable 0.014792 ms speedup. All four runs reproduce the
same behavior/episode counts for their corresponding policy.

| Seed / IQ / boundaries | Picked/spawned before → after | Expired before → after | Kills/self/wall before → after | Mean ms before → after |
|---|---|---|---|---|
| 73 / 100 / deadly | 18/20 → 18/21 | 2 → 3 | 13/3/0 → 18/2/0 | 0.35290 → 0.38070 |
| 73 / 100 / wrap | 21/23 → 21/21 | 1 → 0 | 10/1/0 → 13/2/0 | 0.35110 → 0.34380 |
| 73 / 50 / deadly | 20/22 → 16/20 | 2 → 3 | 15/2/0 → 13/3/0 | 0.39825 → 0.33810 |
| 73 / 50 / wrap | 19/21 → 21/22 | 1 → 1 | 12/1/0 → 11/0/0 | 0.34490 → 0.35210 |
| 20260814 / 100 / deadly | 20/23 → 18/21 | 3 → 2 | 20/0/0 → 16/3/0 | 0.36785 → 0.34605 |
| 20260814 / 100 / wrap | 18/20 → 16/20 | 1 → 3 | 12/2/0 → 8/2/0 | 0.37935 → 0.32985 |
| 20260814 / 50 / deadly | 14/22 → 19/22 | 6 → 3 | 11/3/0 → 20/1/0 | 0.38240 → 0.37225 |
| 20260814 / 50 / wrap | 21/22 → 20/22 | 1 → 2 | 12/2/0 → 14/3/0 | 0.37295 → 0.33035 |
| 991 / 100 / deadly | 18/21 → 14/20 | 2 → 6 | 21/2/0 → 10/3/0 | 0.36100 → 0.34595 |
| 991 / 100 / wrap | 15/20 → 20/21 | 4 → 1 | 17/0/0 → 16/0/0 | 0.35610 → 0.34585 |
| 991 / 50 / deadly | 18/21 → 20/21 | 3 → 0 | 16/1/0 → 16/0/0 | 0.33085 → 0.33355 |
| 991 / 50 / wrap | 15/22 → 20/22 | 7 → 2 | 11/3/0 → 12/1/0 | 0.35190 → 0.35350 |

Individual fixtures are mixed: e.g. seed 991/IQ100/deadly collects 14/20, below
75%. The requested pickup, combat and timing gates are aggregate twelve-case
gates; no per-case guarantee is claimed.

### Focused duels and verification

Each selected duel crosses 160 first-generation fixtures: IQ100/50,
aggression1.0/0.15, ten offsets, mirror and snake-ID swap. Both policies run
sequentially on CPU8. `limited` uses `--scripted-prey`; responsive uses the
ordinary rival controller. These are focused competition regressions, not a
dedicated capsule-race success metric.

| Scenario / prey | Designated kills before → after | Other-owner kills before → after | Attacker deaths before → after | Victim self/wall before → after |
|---|---:|---:|---:|---:|
| food_cutoff / responsive | 0 → 0 | 0 → 0 | 1 → 1 | 0/0 → 0/0 |
| food_cutoff / limited | 67 → 67 | 27 → 27 | 0 → 0 | 0/0 → 0/0 |
| feasible_crossing / responsive | 1 → 1 | 0 → 0 | 1 → 1 | 0/0 → 0/0 |
| feasible_crossing / limited | 57 → 56 | 0 → 0 | 6 → 6 | 0/0 → 0/0 |

Toolchain: `/usr/bin/cargo` and `/usr/bin/rustc` 1.93.1. Focused release checks
ran with frozen/offline dependencies; no rustup or network was used:

```sh
env RUSTC=/usr/bin/rustc RUSTDOC=/usr/bin/rustdoc /usr/bin/cargo test --release --frozen --offline --manifest-path rust/snakes-core/Cargo.toml --lib ai:: -- --test-threads=1
env RUSTC=/usr/bin/rustc RUSTDOC=/usr/bin/rustdoc /usr/bin/cargo test --release --frozen --offline --manifest-path rust/snakes-core/Cargo.toml --test allocation zero_allocations -- --test-threads=1
env RUSTC=/usr/bin/rustc RUSTDOC=/usr/bin/rustdoc /usr/bin/cargo test --release --frozen --offline --manifest-path rust/snakes-core/Cargo.toml --test ai_boost_allocation -- --nocapture
env RUSTC=/usr/bin/rustc RUSTDOC=/usr/bin/rustdoc /usr/bin/cargo test --release --frozen --offline --manifest-path rust/snakes-core/Cargo.toml --example ai_scorecard -- --test-threads=1
env RUSTC=/usr/bin/rustc RUSTDOC=/usr/bin/rustdoc taskset -c 8 /usr/bin/cargo build --release --frozen --offline --manifest-path rust/snakes-core/Cargo.toml --example ai_scorecard --example ai_competition
/home/mjstanton/.local/bin/heavy bash /home/mjstanton/.cache/agent-scratch/plasma-wayland-screensaver/pursuit/paired.sh
/home/mjstanton/.local/bin/heavy bash /home/mjstanton/.cache/agent-scratch/plasma-wayland-screensaver/pursuit/duels.sh
git diff --check
```

Results: **140 AI tests**, **5 allocation fixtures**, **1 public AI/boost
allocation test**, and **8 scorecard/accounting/contact tests** passed.
Allocation checks measured **zero allocations and reallocations**, including
first control, steady-state ticks, reconfiguration, capsules, effect replacement/
expiry, deaths/respawns and simultaneous effects. Both release examples build.
Both paired matrices and all eight selected duel runs finish successfully.
`git diff --check` passes. Eight new focused policy tests cover capsule priority,
turning, expiry/commitment, effect context/replacement, races, contender intent,
generation validation, stale hunt steering, safe pickup and Surge burst gates.

The scripts execute `taskset -c 8 <ai_scorecard> 8 --ai-only --capsule-trace`
four times in before/after/after/before order; duels execute
`taskset -c 8 <ai_competition> --scenario <food_cutoff|feasible_crossing>`
with/without `--scripted-prey` for both policies. The instrumented baseline
examples were built with the same build command using the baseline manifest.
Raw logs/scripts and the baseline source are retained under
`/home/mjstanton/.cache/agent-scratch/plasma-wayland-screensaver/pursuit/`:
`paired-{1,2}-{before,after}.txt`, `duel-*-{before,after}.txt`,
`ai-tests-final.txt`, `allocation-final.txt`, `boost-allocation-final.txt`, and
`observer-tests.txt`. The temporary baseline build directory is removed after
measurement; rebuilding its examples makes the scripts runnable again.

No full Cargo suite, native build/CTest or desktop visual check ran in this
worker: the brief assigns full builds/suites to CI and limits local verification
to focused checks. No commit, push, branch operation or review loop ran. No
temporary servers/watchers remain. The orchestrator still needs review and CI.

## 0.11.0 step A: faces, landing and race hooks

ABI v3 now exports deterministic moods, capped persistent bubbles and capsule
race/guard state, while reserving the later 0.12.0–0.17.0 effect/event data.
Capsules announce for 30 ticks before pickup, and all effect/contact forecasts
respect that endpoint. New commitments use the 0.9/1.25 ETA gates; aborts need
ten consecutive losing ticks against a committed live rival. Nearby denial
creates one generation-guarded 150-tick grudge; safety still controls all chases
and cutoffs. Held-effect guards reuse orbit steering and release at 30 ticks.
Mechanics, opportunities and rollout awards share `Item::pickup_eligible`,
including landing, lifetime and guard clocks. Cached contests store contact
only; each candidate checks eligibility in item order and clears guards when
another snake consumes their target. Own rollouts use the current guard intent
before the world's face export. Intentional guard turns do not accumulate
anti-circling recovery debt; race/grudge coils already have the coil exemption.
The observer forwards face intent, so scorecards include these production
rules. `--user-settings` selects density 30, trails 100, scale 185 and speed 230;
pass IQ 100/deadly to reproduce the user's collision settings.

See [R11_STEP_A_REPORT.md](../../R11_STEP_A_REPORT.md) for focused verification,
measurements and the ABI handoff to renderer workers. The preceding pursuit
section records the staged baseline and its historical measurements.

## S2: planned prism prizes (0.12.0)

A single prize is cached during preparation, independently of the 64-food
shortlist budget. Ripe Prism has strategic AI value 90 (nutrition remains 5);
the seed uses `90 / (1 + remaining_seconds * .35)`. Trailing contenders retain
interest rather than inheriting ordinary-food loser penalties. Discovering a
prize respects recovery rejection, and empty shortlist slots do not hide food
index zero. Contact forecasts wait until the same endpoint as feeding and
vacuum claims; overlapping an unripe target never proves collection.

Early arrivals reuse the guard/coil tangent planner with radius
`max(5.5 * radius, 1.35 * speed / turn, speed * .7 seconds,
1.1 * min(body_length, speed * 3 seconds) / TAU)`. Body length includes
the 1.18-radius segment spacing. This leaves clearance for the short wait,
without demanding that every snake fit its entire body into a closed ring. Approach handedness is
chosen from the current radial/head-heading cross product and held during the
orbit. Its center starts one orbit radius to that side of the actual head,
so entry is tangent to the current heading and the approaching prize can lie
near the circle edge. Radial correction scales with the orbit. The existing collision and
room gates still choose movement. Intentional vulture turns are exempt from
circling debt and stalled-target rejection. An established wait lasts until
the final six countdown ticks; release clears old turn-direction hysteresis
and commitment. Rollouts include the timed transition from orbit to pounce;
they never reject a short wait on the assumption it will orbit forever. Nearby
prizes use moving-body rollouts in place of static route rejection; a ripe
prize gets four seconds to make progress before ordinary stall recovery.
Safe forecasted captures receive a 600-point utility bonus,
which ranks below safety and available space.

Close races use turn-aware rival ETAs within 25% to propose a checked boost.
A late seed pounce can propose that boost only when its estimated boosted
arrival does not precede ripening. Larger prize contenders reuse capsule
cutoffs while keeping the direct prize route as a candidate. Ripening ends
retained ambush controls immediately, including between strategy slots, and
ripe tactical revisions keep the prize pounce. Existing hunters
give prey within 12 prey radii of a seed a 100% score premium; a cutoff with
verified blocked responses near the seed values each response at 250 rather
than 150. Size, boost-payment, Phase, collision and room gates are unchanged.

Prism placement uses the same twelve XY RNG draws as capsule placement but
prefers live heads without capsule commitments (falling back to all live
heads), then projects candidates onto their approach-side circles at 3.1–3.5
seconds of travel. This puts the first pass just after ripening. Candidates require six base radii of continuous-body clearance;
the placement score favors the earliest two turn-aware arrivals. Capsules
retain their uniform maximum-clearance placement, and Classic still takes no
Prism scheduler draws. Countdown, nutrition, ripe lifetime and shared pickup
eligibility are unchanged.

The scorecard reports spawned/picked/pending counts, ripe-to-eaten median,
contested seeds, how many had at least twenty degrees of intentional orbit,
vulture episodes/full turns/snake-seconds, and kills within 12 victim radii
of seeds and of either prize phase. `--prism-trace` prints exact per-prize
delays for pooled multi-seed medians. `--prism-motion-trace` prints the first
prize's live heads, goals, orbit state and safe forecast endpoints. Observer
work and its allocations remain outside timed ticks. See
[R12_PRISM_REPORT.md](../../R12_PRISM_REPORT.md) for sample counts and gates.

## S3 Venom

`venom.rs` refreshes a holder's hunt immediately and on each held tick. Eight
severable body samples per rival compete by cut value, rival size, turn-aware
arrival before expiry, approach clearance and target retention. A bounded
256-record query of the existing body grid also finds nearby titan segments
between those samples. Useful Venom capsules value these reachable body points.
The old minimum rival-size and fixed search-distance gates no longer discard
smaller or distant but reachable cuts.

The primary strike, another rear-body point and the nearest feasible strike
compete under the ordinary wall/head/self/deposited-neck rollouts. A checked
burst can close the gap when its tail payment is smaller than the intended cut.
Confronted holders flank the body; defenders still show their head and tighten
coils. Certified bites, including incidental cuts, earn utility below safety
and available space, with earlier bites preferred. Every hunt is exempt from
anti-circling recovery. Target identity is slot plus generation; alternative
candidate commitment carries the whole strike plan, and charge loss clears
stale steering before ordinary strategy resumes.

Mechanics and every body query share `world::venom::bite_eligible`. Candidate
forecasts hold bounded cut, immunity and consumed-charge state; margin-only
contacts never spend a charge. Removed static trail points stop blocking later
steps; retained stumps use their shortened taper and release clock, while newly
deposited necks remain lethal. The three exit ticks after a bite are swept
individually. Goal-tracking strikes continue straight after charge consumption,
so later straight exit sweeps need no own-curvature padding; rival envelopes
and the ordinary body reserve remain. Mechanics and forecasts select the
lowest eligible contacted index on the victim, preventing spatial bucket order
from leaving an already-touched point as a fresh stump. The ordinary
path specializes away bite work and uses a bitmask to skip Venom effect lookup.
Venom bypasses the ordinary body cache because its cut/charge state is path-local.
Defenders walk the observed holder mask from preparation; ordinary ticks do
not build every rival view merely to reject its effect kind.

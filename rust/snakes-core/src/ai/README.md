# Rust snake AI — iteration 2

Iteration 2 cuts total deaths from **988 to 195 (80.3%)** across twelve paired
8-minute ecosystems. The old-JS reference configuration has **14 deaths**, versus
Rust V1's 79 and the supplied JS result's 64. All twelve runs meet the total-death,
wall, oscillation, average-time and p99 targets. **The self-death target remains
unfinished:** ten configurations exceed five self deaths, with 3–13 per run.
Remaining deaths cannot mostly be described as successful rival outplays: 104 of
195 are self hits. This is a substantial improvement, not full behavioral acceptance.

The paired V1 source is the pre-task staged implementation, against original
project base `5f4f1d9a07ac6292a668868aaf797af68f105802`. Its saved Git blobs are
`src/ai/mod.rs`: `2cde574589eefa3e64ef8bf3028ecea0c0e6989c`, and
`src/ai/spatial.rs`: `80d5ed8758f5b3924a6dd5691ab8bc94aab4a544`.
Before diagnostics use an isolated non-Git copy of that algorithm with the same
observer/replay harness. Existing staged mechanics, integration, ABI exports and
layouts are preserved. This iteration does not change `world.rs`.

## What changed and why

V1's reference diagnosis had 20 rival-prediction misses and 21 space-estimation
hypotheses among 78 non-head deaths. Its short mandatory horizon, three endpoint
fills and execution/rollout disagreement missed traps. Two concrete planning
bugs were especially harmful: a turn-then-straight rollout was executed as a
continuing hard turn, and rival forecasts ignored retained turn deadlines.
Truncated rollouts also lost their intended exit heading. These now retain the
same control schedule through rollout, execution, continuation and known rival
prediction. Turn commitment covers its expected duration rather than expiring
mid-maneuver.

All safety rollouts now reach 72 ticks (2.4 seconds), irrespective of IQ. A body
check verifies the first tick separately, then swept groups of at most four
exact motion steps with curvature padding. A candidate's safe count advances
only after a complete swept check; an unchecked interval is never reported as
survived. First-tick body queries include observed previous/current body motion
and enlarge the broad phase by the maximum observed motion. Later static trails
use conservative time-to-tail-release plus the current growth reserve's actual
stretch-cycle delay. Forecast future rival neck deposition is checked as well
as losing/tied head contests and the candidate's own new trail.

Every evaluated candidate gets an area estimate at its checked endpoint and
arrival time. Tail release affects those estimates. A one-cell obstacle band
rejects cracks without turning room; a narrow starting cell can traverse up to
two physically free cells to reach wide space. Such root-specific searches are
not cached as connectivity for other roots. Pocket recovery targets the tail,
ends when present space opens, and can be interrupted by sustained-turn escape.
Analytic wall viability rejects states unable to turn parallel to an approaching
wall even when the crossing lies beyond the horizon. A wall-room utility favors
room for later turns. Plan reuse is disabled near walls.

The extra horizon and area work are paid for by cheaper motion/geometry and
shared work: incremental sine/cosine rotations, tight swept cell ranges, axis
rejection, wrap only when needed, a 256-entry per-tick body-query cache, connected
area reuse, and adaptive fill limits. An existing plan is checked against current
bodies and forecasts every tick. If fully safe and open (or committed), it can
be retained between scheduled strategy slots without evaluating eight redundant
alternatives. Diagnostics evaluate those alternatives but preserve the original
choice; a 5,000-tick regression checks that observation does not change worlds.

Food scoring rewards closest approach along the path, rather than penalizing a
route whose endpoint has already passed the food. Size advantages, food races,
cluster/feast value, rival interception and pursuit remain active. Winning head
contact and winning food contest/cutoff fixtures verify concrete competitive
behavior. Intelligence still influences strategy and aggression, but no longer
reduces mandatory safety coverage.

## Algorithm, bounds and knobs

One deterministic decision chooses among nine controls: goal heading, retained
plan, straight, gentle left/right (±0.6 radians), hard left/right (±3 radians),
and hard turn for 16 ticks then straight. Motion uses the actual read-only
mechanics speed/turn limits with rush zero. Safety duration wins lexicographically.
If none survives the horizon, checked duration then clearance precede utility.
Otherwise utility combines food approach, heading, clearance, space, continuity
and turn bias. Known insufficient pockets get a large penalty. Full-horizon
safety is a bounded prediction, not a proof of indefinite survival.

One AI-owned grid is rebuilt per observed world tick, O(cells + live segments +
food); queries use its linked body records, food buckets and occupancy masks.
This remains a full rebuild: the mechanics moves **every body sample**, including
corner relaxation and changing spacing/radius. Updating only head/tail cells
would leave stale obstacles. A correct incremental grid would need to inspect
all moved samples and maintain deletion links, release maxima and dilation; that
migration was not justified by the measured budget. Mechanics' own grid remains
private. Fixed preallocated storage uses a few MiB per controller and is retained
across configuration changes. Steady-state ticks allocate nothing.

- `STEPS=72`, `CANDIDATES=9`, `STRATEGY_QUOTA=2`, `URGENT_QUOTA=2`.
- `NARROW_LIMIT=4096` visited body records per candidate. Query-cache hits charge
  the same visits as the original query. Reaching the cap rejects unverified
  continuation and sets uncertainty.
- At most 128×128 grid cells and 14×1600 body slots. Cell side is
  `max(28, base_radius * 5)`, with a one-cell turn-space clearance band.
- Required area is `max(24, ceil(length * radius² * 8 / cell_area))`.
  Each BFS stops at `clamp(max(64, 2 * required_area), 1, 512)` cells. Capped areas
  are lower bounds, not exact sizes. Cache keys include mask, limit, time and
  rebuild epoch; connected roots share results only when sound.
- Tail travel assumes 65% current speed, segment spacing `radius * 1.18`, and a
  0.15-second reserve. Growth delay uses remaining growth increments/stretch and
  the mechanics' 0.62 growth-stretch factor; delay is zero at snake/world cap.
- Food discovery examines at most 1024 cells and 64 particles, retains five
  targets, and routes with at most 512 cells. Larger snakes require six excess
  segments to accept head contests, eight for tactical pursuit.
- Retained-plan utility bonus is 90, with an extra 160 during commitment.
  Pocket penalty is `600 + 8 * missing_cells`. Turn-sign reversal costs 35.
  Tail recovery lasts up to 45 ticks; sustained turn >2.8 radians triggers a
  36-tick forward escape; no food progress for 75 ticks rejects that target.

Strategy covers two ordinary snakes round-robin plus at most two urgent snakes
per tick. At maximum population, ordinary coverage is within seven ticks. Every
snake still revalidates safety each tick. Worst-case work remains bounded by
population × nine 72-step rollouts, four strategy searches, per-candidate body
visit limits and nine bounded fills per snake. Rival forecasts are shared.
No wall-clock timing affects decisions; the controller owns an independent RNG.

## Paired ecosystem measurements

Intel i9-13900K; separate P cores 8/10/12/14, four sequential job lanes, with no
concurrent task compiler or heavy observer jobs during the final normal matrix.
Each run is 3440×1440, density/trails 100, scale/speed 100, self collisions on,
30 Hz, eight simulated minutes. Timings include AI **and mechanics**, exclude
scorecard observation and rendering. Host/user processes were left alone.
The supplied JS timing (~3.35 ms/tick) has different controller-induced geometry
and timing resolution, so it is context rather than a same-state microbenchmark.

| Seed | IQ | Walls | Deaths V1 → V2 | Self V1 → V2 | Wall V1 → V2 | Avg ms V1 → V2 | p99 ms V1 → V2 | Oscillations/snake-min V1 → V2 |
| --- | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 73 | 100 | deadly | 70 → 17 | 21 → 6 | 8 → 0 | 0.4323 → 0.2517 | 1.2314 → 0.4098 | 17.65 → 7.53 |
| 73 | 100 | wrap | 82 → 16 | 33 → 11 | 0 → 0 | 0.4860 → 0.2913 | 0.7035 → 0.4740 | 21.99 → 6.44 |
| 73 | 50 | deadly | 88 → 18 | 34 → 12 | 5 → 0 | 0.3387 → 0.2560 | 0.5012 → 0.3980 | 19.85 → 6.94 |
| 73 | 50 | wrap | 79 → 18 | 26 → 12 | 0 → 0 | 0.4624 → 0.2887 | 0.7113 → 0.4670 | 19.64 → 6.37 |
| 20260814 | 100 | deadly | 79 → 14 | 27 → 6 | 2 → 0 | 0.3808 → 0.2645 | 0.6065 → 0.4253 | 22.73 → 6.70 |
| 20260814 | 100 | wrap | 82 → 15 | 33 → 8 | 0 → 0 | 0.5177 → 0.2881 | 0.7403 → 0.4758 | 21.35 → 6.74 |
| 20260814 | 50 | deadly | 87 → 18 | 30 → 4 | 4 → 0 | 0.3308 → 0.2588 | 0.5148 → 0.4158 | 20.43 → 5.80 |
| 20260814 | 50 | wrap | 75 → 15 | 30 → 9 | 0 → 0 | 0.4772 → 0.2881 | 0.7050 → 0.4863 | 20.19 → 6.79 |
| 991 | 100 | deadly | 89 → 18 | 39 → 13 | 0 → 0 | 0.3802 → 0.2532 | 0.6401 → 0.4072 | 21.82 → 7.08 |
| 991 | 100 | wrap | 86 → 13 | 33 → 8 | 0 → 0 | 0.6025 → 0.2850 | 1.0812 → 0.4659 | 22.42 → 5.77 |
| 991 | 50 | deadly | 97 → 18 | 39 → 12 | 2 → 0 | 0.3331 → 0.2483 | 0.4841 → 0.3881 | 21.82 → 7.08 |
| 991 | 50 | wrap | 74 → 15 | 31 → 3 | 0 → 0 | 0.5206 → 0.2954 | 0.9328 → 0.4881 | 20.47 → 7.87 |

Totals: **988 → 195 deaths**, **376 → 104 self deaths**, **586 → 91 body deaths**,
**21 → 0 wall deaths**, **5 → 0 head deaths**. Mean of per-config average step
times: **0.4385 → 0.2724 ms (37.9% lower)**. Final averages range 0.2483–0.2954 ms,
p99 0.3881–0.4881 ms, oscillations 5.77–7.87 per snake-minute. Reference:
**79 → 14 deaths; 0.3808 → 0.2645 ms average; 0.6065 → 0.4253 ms p99**.

The colony stays larger on average: across-config mean length **68.29 → 74.70**.
Absolute contest wins fall **21079 → 5124**, and estimated bigger kills **359 → 43**.
Fewer deaths produce far fewer corpse particles: food/min averages
**1038.41 → 447.71**. Thus absolute contests/kills are not comparable independent
of food supply and rival mortality. Competition remains measurable (324–569
contest wins and 1–7 estimated bigger kills per run), but these results do not
prove stronger adversarial tactics. Further aggressiveness must be evaluated
without restoring fatal trapping.

| Seed | IQ | Walls | Food/min | Contests won/lost | Bigger kills | Mean/max length | Circling snake-seconds |
| --- | ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 73 | 100 | deadly | 437.38 | 324/340 | 1 | 70.35/159 | 5.00 |
| 73 | 100 | wrap | 469.12 | 440/466 | 3 | 79.33/203 | 0.00 |
| 73 | 50 | deadly | 455.25 | 388/405 | 2 | 72.68/205 | 15.00 |
| 73 | 50 | wrap | 471.50 | 421/446 | 5 | 77.96/245 | 5.00 |
| 20260814 | 100 | deadly | 402.25 | 455/487 | 5 | 73.73/208 | 5.00 |
| 20260814 | 100 | wrap | 450.75 | 442/459 | 4 | 76.53/209 | 15.00 |
| 20260814 | 50 | deadly | 465.50 | 381/406 | 7 | 71.72/217 | 45.00 |
| 20260814 | 50 | wrap | 450.38 | 569/599 | 4 | 75.70/242 | 5.00 |
| 991 | 100 | deadly | 426.88 | 336/347 | 1 | 70.89/203 | 10.00 |
| 991 | 100 | wrap | 438.50 | 435/451 | 2 | 76.79/198 | 5.00 |
| 991 | 50 | deadly | 430.62 | 371/385 | 3 | 70.11/208 | 40.00 |
| 991 | 50 | wrap | 474.38 | 562/597 | 6 | 80.66/199 | 15.00 |

Death totals/reasons come from mechanics. Food disappearance, physical near-food
contests, bigger-killer attribution and movement-style metrics are observer
estimates, not event counters or proof of planned cutoffs. A contest counts a
near-food race with another live head within 160 px; each particle counts one
winner and all challengers as losers. Expiry/eviction is filtered by nutrition
increase near capture. Oscillation is a turn-sign change within 15 ticks with
>0.008-radian movement. Circling uses a five-second window with little positional
or nutritional progress. No live Wayland visual soak was run by this worker;
native/QML integration checks cover plumbing, not subjective animation quality.

## Death diagnostics

`--diagnostics` records nine candidate controls, checked horizons, estimated
area/cap, required area and length at exact 1/2/3-second lookbacks. It saves five
world/controller snapshots at one-second intervals, normally replays from
60–89 ticks before death, and tries all nine alternatives. Each probe holds its
alternative for one second (respecting turn-then-straight/dynamic V1 goal
semantics), then resumes the planner; all rivals continue responding normally.
A probe must survive four seconds from its snapshot. Snapshot allocation/cloning
is diagnostic-only and never runs in normal ticks.

The final paired matrix covers **983 V1 and 195 V2 non-head deaths**, with no
missing snapshots. Every non-timing scorecard field matches the corresponding
normal run for all twelve before and all twelve after configurations.

| Replay diagnosis | V1 | V2 |
| --- | ---: | ---: |
| No tested replay escape | 417 | 151 |
| Late detection | 11 | 1 |
| Bad pocket, another measured open survivor | 7 | 2 |
| Fill underestimate hypothesis | 107 | 32 |
| Fill overestimate/turnability hypothesis | 142 | 5 |
| Rival prediction hypothesis | 299 | 4 |
| Query cap observed | 0 | 0 |
| Missing snapshot | 0 | 0 |

| Actual death reason | No replay escape V1 → V2 | Underestimate V1 → V2 | Overestimate/turnability V1 → V2 | Rival prediction V1 → V2 | Bad pocket V1 → V2 | Late V1 → V2 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Self | 160 → 80 | 70 → 18 | 142 → 5 | 0 → 0 | 4 → 1 | 0 → 0 |
| Other body | 246 → 71 | 37 → 14 | 0 → 0 | 299 → 4 | 3 → 1 | 1 → 1 |
| Wall | 11 → 0 | 0 → 0 | 0 → 0 | 0 → 0 | 0 → 0 | 10 → 0 |

Reference (seed 20260814, IQ100, deadly): V1's 78 diagnosed deaths are
36 no replay escape, 1 late, 10 underestimate, 11 overestimate/turnability and
20 rival prediction. V2's 14 are 12 no replay escape and 2 underestimate.

These labels are **hypotheses, not ground truth**. No replay escape means only
that these nine one-second control probes failed; it cannot establish that a
snake was unavoidably boxed in by others. A surviving probe demonstrates an
escape under reacting rivals, but later decisions can diverge. The classifier
uses the snapshot's selected area and available survivor masks: low selected
area with a measured-open surviving option is a bad-pocket hypothesis; low area
with other survivors suggests underestimation; self death with enough estimated
area suggests overestimation/turnability. A body death with a full predicted
horizon suggests rival-prediction miss. Cap classification has precedence if
any candidate is capped; it would not by itself prove that the cap caused death.
No such query caps were observed here.

The remaining self issue is visible before impact: **all 104 self deaths had no
72-tick candidate at both one and two seconds before death**, although 76 had a
full-horizon candidate at three seconds. Eighty have no surviving replay probe
from the 2–3-second snapshot. This supports earlier enclosure detection and
multi-maneuver planning rather than simply stronger last-tick avoidance. It does
not distinguish self-created enclosure from a rival forcing the escape closed,
and does not prove the strict target impossible.

Raw task logs are in `/tmp/ai2-results`: `paired-before-*`, `accepted-*`,
`diagnostic-before-*`, `diagnostic-after-*`, `cargo-accepted.txt`,
`cmake-accepted-build.txt`, `ctest-accepted.txt`, and `stress-accepted.txt`.


## Reproduction and verification

Use the distribution toolchain without network/rustup. From the repository root:

```sh
RUSTC=/usr/bin/rustc /usr/bin/cargo test --frozen --offline --manifest-path rust/snakes-core/Cargo.toml
RUSTC=/usr/bin/rustc /usr/bin/cargo build --release --frozen --offline --manifest-path rust/snakes-core/Cargo.toml --example ai_scorecard
cmake -S . -B build-ai2 -G Ninja -DCMAKE_BUILD_TYPE=Release -DBUILD_TESTING=ON
cmake --build build-ai2 -j 6
ctest --test-dir build-ai2 --output-on-failure
RUSTC=/usr/bin/rustc taskset -c 8 /usr/bin/cargo test --release --frozen --offline --manifest-path rust/snakes-core/Cargo.toml --test allocation zero_allocations_with_ai_at_caps_and_after_reconfiguration -- --nocapture
cc -std=c11 -Wall -Wextra -Werror -I rust/snakes-core/include rust/snakes-core/tests/ai_debug_smoke.c build-ai2/cargo-target/Release/release/libsnakes_core.a -ldl -lpthread -lm -o /tmp/ai2-debug-smoke
/tmp/ai2-debug-smoke
taskset -c 8 rust/snakes-core/target/release/examples/ai_scorecard 8 20260814 100 deadly --ai-only
taskset -c 16 rust/snakes-core/target/release/examples/ai_scorecard 8 20260814 100 deadly --ai-only --diagnostics
```

Matrix job order is seeds `[73, 20260814, 991]`, IQ `[100, 50]`, walls
`[deadly, wrap]`; lane `n` runs jobs `jobs[n::4]` sequentially on
`[8, 10, 12, 14][n]`. Before/after diagnostics run each configuration on its own
E core 16–27, before then after. Never use diagnostic or profiling timings for
budget acceptance. `--help` lists optional positional arguments and flags.

The latest Cargo suite passes **90 tests** (42 library, 44 in the private
allocation harness including duplicated library tests, four integration tests).
Full CTest passes **14/14**, including `snakes-parity` and
`snakes-parity-record`. The release example and C debug client build/run cleanly;
`--help` and `git diff --check` pass.

The pinned release cap fixture holds **6000 segments and 450 food** at start,
retains **6000 segments after 1000 measured ticks**, then checks a further
2000-tick self-collision/deadly reconfiguration. Both measured phases have
**zero allocations/reallocations**, including first AI ticks. First-phase
average is **0.4741 ms**, p95 **0.5297**, p99 **0.5611**, maximum **1.0169**.
V1 measured **0.7078 ms average**, p99 **0.9303**, ending at 5710 segments.
The denser scale-25 stress average now meets the ≤0.6 ms target while preserving
its full segment population. The normal ecosystem's average/p99 targets are
also met in every configuration, not obtained by measuring diagnostic mode.

New regression coverage includes plan execution/continuation/exit deadlines,
known rival plans, truncated exits, observed body-motion broad phase, exact query
cache results and cap accounting, tail growth delay/cap, turning-space release,
area invalidation, recovery cancellation, winning contact/cutoff, independent
snapshot replay, and non-invasive diagnostics/profiling. Existing allocation,
ABI, scripted-control, determinism and JS mechanics fixtures remain green.
No ABI layout/function/version changes; the header only adds a comment for debug
flag bit 4 (16), which marks a revalidated retained plan.

## Remaining work

- **Self hits ≤5/run is unmet in ten configurations.** Coarse connectivity and
  a 2.4-second single-maneuver horizon cannot reliably preserve an escape through
  a growing self-made enclosure. Need trajectory-linked future occupancy and
  multi-maneuver escape evaluation, with new perf evidence. The present data does
  not prove the target infeasible, and unsuccessful replay probes do not justify
  calling these deaths unavoidable.
- Area is measured at candidate arrival, not continuously along every path;
  neck/own-trail occupancy and coarse dilation can underestimate room, while
  connectivity cannot prove curvature-constrained escape. Tail-growth/speed/radius
  estimates remain conservative forecasts, especially when more food is eaten.
- Rival plans may change next tick. The bounded widening envelope and known
  turn schedule reduce misses but cannot predict adversarial decisions exactly.
  Winning attacks still pass survival guards that assume the opponent persists.
- Head/tail-only grid updates would be incorrect for these mechanics; a correct
  moved-sample incremental index is still open if future profiling warrants it.
- Opportunistic interception is not adversarial search or explicit body-placement
  optimization. Lower mortality changes food supply; contest strength needs a
  controlled independent benchmark as well as the natural ecosystem.
- No subjective live screensaver visual validation. Review/publication are left
  to the orchestrator. No commits or review loops were run by this worker.

All task-started one-shot processes have finished. No servers/watchers remain.

# Starfall and Nightfall (0.17.0)

World events are enabled by default for V2 Snakes. The redesigned Appearance
page exposes the existing `snakeWorldEvents` binding through
`Catalog.snakeWorldEventsAvailable`. Classic keeps its original RNG draws,
trail bounds and parity trace. The C ABI stays at v3.

The scheduler, AI objective and sprites live in `world/events.rs`,
`ai/events.rs` and `render/events.rs`. They use preallocated storage and
absolute u64 deadlines. Starfall intervals are 5400–9000 ticks; Nightfall
intervals are 12600–18000 ticks. Scheduling draws randomness only when an
interval or shower is planned.

Starfall considers 48 projected approach candidates, alternating nearby pairs
and centres of four nearby heads. It ranks four arrival times, retains the
shared six-base-radius continuous-body clearance and capsule spacing, and
reserves a six-radius exit lane beyond the landing disk at deadly walls.
A successful announcement samples one diagonal and 24 uniform disk landings.
The zone radius is 12 base radii; a 45-tick telegraph precedes alternating
3/2-tick launches across 60 ticks. Each meteor flies for 14 ticks from 34 base
radii away, then becomes a value-1 Star with a 900-tick active lifetime.

The fixed extra target slot is worth 7 strategic units, using the existing
prism normalization. Its race reuses aggression, commitment, rejection,
boost and certified cutoff handling. Since a shower has 24 prizes, a trailing
racer can join within a two-second allowance, rising to six at full aggression.
Once stars land, heads arriving in the disk release the zone to sweep ordinary
food; distant heads can continue their approach while stars remain. Recovery,
Frost and other tactical goals retain priority over suspended objectives.
The zone is never a collectible capsule or a held-capsule guard orbit.
Meteor and captured-food eligibility is shared by mechanics and forecast
contacts. No in-flight meteor can be collected or enter food discovery buckets.

Nightfall lasts 750 ticks with 90-tick fades. It can overlap Starfall. Its
speed factor, `1 - 0.1 * night`, applies to actual motion, cached single-burst
forecasts, public schedules and effect/Frost replacement forecasts. Forecasts
follow the known fade and scheduled onset; dawn broad queries reserve daylight
speed/curvature. Night intensity invalidates the motion cache. Target-less
snakes doze while hunting and higher-priority moods continue.

The shared material dims tube/sheen to 0.28, retains silhouette glow and adds
iris shine. Food halos reach 1.6 times normal brightness. Clock and scrim are
outside SnakeMaterial. Kinds 20/21/22 draw the ring, stars with landing puffs,
and meteor streaks/heads. Event food shares colour and scale setup across a
shower; ordinary food bypasses that path. Reduced motion holds procedural
time and fades meteors at landing spots. Landing puffs age on simulation time
and use a 0.6 duration in Calm. Mono and Pastel use existing accent rules.

`Snake.face_flags` preserves layout: bit 3 is a committed Starfall head,
bits 4/5 mark the nearest two, bit 6 the ETA leader, and bit 7 a race within
18%. Contest geometry retains the global six-arc budget. WorldEvent markers
use `other_snake_id` 1/2, nonzero duration for start and zero for end. Night
intensity and ambient remain independent of the overlapping shower payload.

## Verification

Use distribution Rust 1.93, offline/frozen. Full suites, builds and measurement
matrices run through `heavy`; focused checks can run directly.

```sh
RUSTC=/usr/bin/rustc RUSTDOC=/usr/bin/rustdoc /usr/bin/cargo test \
  --manifest-path rust/snakes-core/Cargo.toml --release --frozen --offline --lib events
RUSTC=/usr/bin/rustc RUSTDOC=/usr/bin/rustdoc /usr/bin/cargo test \
  --manifest-path rust/snakes-core/Cargo.toml --release --frozen --offline \
  --test allocation zero_allocations_starfall
RUSTC=/usr/bin/rustc RUSTDOC=/usr/bin/rustdoc /usr/bin/cargo test \
  --manifest-path rust/snakes-core/Cargo.toml --release --frozen --offline \
  --test render event_sprites
```

The scorecard reproduces F1's 3440→5360→7920 by 1440 startup sequence,
9000-tick warm-up and 54000 measured ticks, at density80, scale200, speed300,
trails100, IQ100 and default aggression100 with deadly walls/self collisions.
Seeds are 1/73/991. Participants are distinct snake slots entering the disk
while its stars remain. Telegraph commitments count distinct heads selecting
the zone in its 45-tick announcement. Frenzy kills retain the original global
shower-window definition; `zone_kills` separately counts disk-local kills
through the feeding period. Night hunter/sleepy ticks verify mood activity.
CPU comparisons use the identical feature-OFF benchmark on 0.16.0 and 0.17.0,
pinned and serial in B/A/A/B/B/A order, with process CPU time to exclude
scheduler delays. Whole-program static SPIR-V math/comparison instructions
are 1995→2120 (+125, 6.27%); this is not dynamic GPU ISA or frame timing.

## Captures

Qt rows `events` and `events-mono` use exact production geometry at 3440×1440,
base radius18, zone radius216 and meteor travel612. Select
`REAL_EVENTS_PHASE=telegraph|meteors|stars|night`; `REAL_CALM=1` enables Calm.
`SNAKES_VERTEX_DUMP_PATH` exports the production vertices before native window
creation. In a GUI-socket sandbox, the surfaceless EGL harness draws those
vertices with the baked GLSL330 shaders and generated atlas, using Mesa
llvmpipe. This checks shader output, not the native Qt swapchain or real GPU
frame time. Native RHI verification remains a desktop integration check.

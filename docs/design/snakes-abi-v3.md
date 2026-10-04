# Snakes ABI v3: 0.11.0 step A

The ABI is bumped once, from 2 to 3. All v2 record prefixes retain their offsets;
functions and vertex formats retain their signatures/layouts. Consumers must
recompile against the new header and library together. Field-level contracts,
enums, sentinels and C/C++ layout asserts are in
`rust/snakes-core/include/snakes_core.h`; Rust mirrors/asserts are in `src/ffi.rs`.
No new dependency, RNG draw or steady-state allocation is introduced.

| Record | v2 bytes | v3 bytes | Appended data / offset |
|---|---:|---:|---|
| Config | 80 | 80 | Existing word at 76: bit 31 Power-ups off, bit 30 World events off; zero enables both |
| Steering | 24 | 32 | `actions` at 24: bit 0 held Flip request, currently inert; `reserved` at 28 must be zero |
| Snake | 56 | 152 | mood/intensity/age at 56, target/face flags/jaw at 60, look delta/pupils at 64, effect/animation counters at 80, grudge identity/timer at 96, flip tick at 112, two bulges at 120 |
| Food | 48 | 72 | ripe tick at 48, meteor origin at 56, motion countdown/capture owner at 64, reserved flags at 68 |
| Item | 24 | 88 | pickable tick at 24, ETA leader at 32, landing/count/state at 40, two contender IDs at 44, ETAs at 52, guard at 60, radius/value at 64, charge at 72, owner generation/ID at 76/80 |
| Event | 32 | 56 | cut index/life at 32, identities at 36/40, value at 44, release tick at 48 |
| Frame info | 40 | 128 | ambient at 40, bubble count at 44, three bubble records at 48, world event at 88 |
| Bubble | — | 12 | snake ID/generation, age, glyph, zero reserved byte |
| Bulge | — | 16 | start tick, duration, origin segment, fractional widening |
| World event | — | 40 | start/end ticks, zone position/radius, night/ambient, kind/phase/meteor count |

Frame sizes remain 24 bytes; stats remain 56; segments 16; classic vertices 12;
shader vertices 24; AI debug 200. `MAX_ITEMS` is now **4**: three capsules plus one
reserved Whirlpool vortex, while `MAX_CAPSULES` remains **3**. Fixed scratch,
C++ item snapshots and callers using the public macro reserve all four slots.
The fourth slot is unused until Whirlpool ships. Contender capacity is **2**;
bubbles are **3**; bulges are **2 per snake**; the event ring remains **32**.

## Render-worker contract

Moods are Calm=0, Sleepy=1, Hunting=2, Scared=3, Angry=4, Happy=5, Trapped=6,
Dizzy=7, Frozen=8. Higher-priority event/urgent moods enter immediately;
calm/sleep/hunt transitions and exits need six stable observations. Intensity
ramps over six ticks. Mood age saturates at 65535. Boosting uses Hunting eyes
unless a higher-priority mood applies. Happy is 45 ticks after pickup/kill or a
substantial meal (nutrition >=3); `happy_ticks >18` is the first 27-tick blep.
Sleepy yawns last 39 ticks with a deterministic per-life interval of 150–360.

`look_x/y` is the wrapped **world-space delta**, not a point. `pupil_x/y` is the
head-local offset in radii. `target_item` is a compact live array slot, or 255;
never persist it across frames. `face_flags &1` means guarding; bit 1 (value 2) marks an authoritative mood, including Calm=0.
Without that bit, zero mood uses legacy flag-derived expressions. Snake identity
always includes generation; grudge fields include the culprit's generation.
Future effects may fill frozen/dizzy/immunity/stump/breath/flip/jaw/bulge fields
without another record change. STRIKE=256 and FLIP_HELD=512 are reserved flags.

A spawn is visible at tick T and first pickable at T+30. Its initial life is
780 ticks, including landing, leaving the original 750 active ticks. It is
neither collectible nor an obstacle during landing. All contact, opportunity,
shared and candidate effect forecasts use the same absolute pickable tick.
`landing_ticks = max(pickable_from_tick - frame.tick, 0)`. Event ticks are
completed endpoints. There is no extra spawn RNG consumption.

Item leader ETA uses `distance/speed + abs(heading error)/actual turn rate`, in
seconds, using current physical motion limits. Leader is the lowest ETA racer;
contender IDs are the two spatially nearest committed racers, ordered by
spatial distance. A leader can therefore be outside that pair. Guarding snakes
are exported separately. Missing IDs are UINT32_MAX and missing ETAs +infinity.
Contested arcs compare the two exported ETAs within 18%. Renderer closeness,
colour, wrap copies, clock exclusion and reduced-motion styling remain visual
work for the next step.

`FrameInfo.bubbles[0..bubble_count]` is the persistent authoritative bubble
snapshot. Emote events also identify glyphs in `other_snake_id`, but event-ring
overflow cannot erase an already live bubble. Glyphs: Alert=0, Question=1,
Anger=2, Sleep=3, Heart=4. Age is 0–44. Only one bubble per snake; non-forced
emissions have a 150-tick cooldown; Anger/Heart replace the owner's bubble.
A full global cap drops new owners even for a forced emission. Death/respawn
invalidates a bubble. Clock avoidance belongs to the renderer.

PrismSeed=4, Meteor=5 and Star=6 extend the existing food kinds. A seed's
`ripe_tick` will be spawn+90; `motion_origin`/`motion_ticks` support meteor
flight. `captured_by` is zero when free and vortex item slot+1 when captured.
Bulges are visual only; duration zero disables a slot. Strength will be <=0.35.
Flip=6 and Whirlpool=7 extend effect kinds; **Vortex item kind=8** is separate
from the holder's effect. Vortex state uses item age/life/charge/radius/value
and owner identity. New events are Emote=8, Flip=9, Feast=10, VortexBurst=11,
WorldEvent=12; Sever already exists and now has cut/release payload fields.
All newly reserved effect/food/event payloads are zero until their release.

World event kind: None=0, Starfall=1, Nightfall=2; phase 0 inactive, 1 telegraph,
2 active, 3 fading. The zone window is independent of night/ambient, allowing
Starfall to overlap Nightfall. Ambient exports 1 in step A. No scheduler or
world event behavior is implemented yet. `SnakeWorldEvents` is persisted by
Configuration, exposed as `snakeWorldEvents` to QML, and defaults on.

## AI decisions

New commitments require ETA <0.9 times the best potential rival ETA, or <1.25
with a ready boost and at least 16 segments. Retained commitments abort only
after ten consecutive losing ticks against a **committed live leader** whose
ETA is <0.8 of their own. Potential non-racers cannot trigger abandonment.
Question bubbles require at least 30 ticks of pursuit.

Race boosts require a rival ETA within 25%, distance <18 radii and heading
error <0.45, with the existing turning-radius and safety checks. Trailing
denials require a reachable point at least three leader radii ahead on its
route; the staged cutoff still certifies the crossing and exit. Only the
nearest committed loser within 13 radii reacts to pickup. Its 150-tick grudge
boosts preferred-prey score by 40% within 30 radii and up to 1.2 times its
length. Head-on length checks, body safety and emergency escape still apply.

Holding an effect >90 ticks can initiate guarding of a valuable nearby item;
guarding persists until <=30 ticks. The orbit reuses existing trajectory
steering and normal safety rollouts. Its radius is six own radii, enlarged as
needed for physical curvature, and rejected if it would exceed seven radii.
This prevents impossible coils at the user's high speed/scale. Guards can
propose checked cutoffs against approaching rivals. Guarded pickups do not
replace the held effect before its last 30 ticks.

Faces live in a fixed World side array; SnakeView borrows them. The hot Snake
record copied by motion forecasts keeps its original size. Race ETA caches are
bounded by four item slots and fourteen snakes. Cold future food payloads are
exported directly from the world record, keeping the copied AI FoodView at most
128 bytes; the AI-only target cache stores at most 64 bytes per entry and
contains only contact/valuation fields. A nonzero-payload FFI test covers this
separation. All production changes use
fixed arrays or existing reserved vectors.

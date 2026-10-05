# Snakes ABI v3: 0.11.0 step A

The ABI is bumped once, from 2 to 3. All v2 record prefixes retain their offsets;
functions and vertex formats retain their signatures/layouts. Consumers must
recompile against the new header and library together. Field-level contracts,
enums, sentinels and C/C++ layout asserts are in
`rust/snakes-core/include/snakes_core.h`; Rust mirrors/asserts are in `src/ffi.rs`.
No new dependency, RNG draw or steady-state allocation is introduced.

| Record | v2 bytes | v3 bytes | Appended data / offset |
|---|---:|---:|---|
| Config | 80 | 80 | Existing word at 76: bit 31 Power-ups off, bit 30 World events off, bit 29 (`0x20000000`) Limit snake length on; bits16..22 aggression+1 (zero selects default100); zero enables power-ups/events and leaves length uncapped |
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

## S2 (0.12.0): reserved fields in use, ABI still 3

Prisms follow **Power-ups** and are V2-only. Classic takes no new RNG draws.
The V2 scheduler draws one integer timer in 1200–1800 ticks at initialization
and re-enable. At expiry, if the slot is free and food has room, it uses twelve
XY candidate pairs for reachable Prism placement, then the existing food phase
draw on success, then draws the next timer. Skipped or impossible spawns draw
only the next timer (plus candidate pairs if placement was attempted). Capsules
retain their uniform maximum-body-clearance placement.
Prism candidates prefer live heads with no capsule commitment, falling back to
all live heads. Each candidate projects onto a head's approach-side circle at
3.1–3.5 seconds of travel, with orbit radius enlarged for physical turning limits
and body length. Candidates require six base radii of continuous-body clearance,
20 base radii from capsules, and the existing ten-base-radius deadly-wall margin;
the score favors the earliest two turn-aware arrivals. Tiny deadly arenas with
no wall-safe area return before candidate draws. No second prize appears while
a seed, ripe fruit or vacuum-claimed fruit exists.

Prisms export through the **food** array, not the item array: PrismSeed=4
ripens for 90 ticks (three seconds at 30 Hz), then becomes Prism=3. Both retain
the same food ID and absolute `ripe_tick = spawn tick + 90`. Seed `motion_ticks`
is the remaining countdown, and `life_fraction` is **progress**:
`round((90 - min(motion_ticks, 90)) * 255 / 90)`. The first ripe endpoint sets
`motion_ticks` to zero and starts exactly 900 ticks of active life; ripe
`life_fraction` becomes the usual remaining-life fraction (255 down to 0). A
vacuum claim locks that lifetime. Nutrition is 5 and exported `size` is
`0.62 * base_radius`; `phase` uses the ordinary food phase draw. Reserved meteor
origin and vortex capture fields remain zero for these prizes.

The 24-byte shader vertex packs both food kinds as sprite kind 8 in `params[0]`;
`params[1]` is the phase normalized modulo 2π and scaled to 0–255;
`params[2]` is `life_fraction`. `params[3] = 255` identifies a seed; ripe fruit uses 0–254 for its interpolated
age since `ripe_tick`, reaching 254 after 30 ticks (18 with reduced motion).
A fixture with `ripe_tick = 0` exports ripe age 254. Sprite, food and item kinds
use separate namespaces. The 12-byte fallback draws the seed/progress ring and
ripe fruit directly from the same food records.

Neither a seed nor its announcement blocks movement or permits a vacuum claim.
All target forecasts and the mechanics use the shared endpoint eligibility
primitive. Prism targets retain food IDs internally; face intent namespaces
those IDs with bit 63, keeping them distinct from capsule IDs.

`face_flags &4` means committed to the one live prism prize. `food_flags`
packs its two spatially nearest committed heads: IDs+1 in bits 0–3 and 4–7
(zero means absent), bit 8 means the second head leads, bit 9 means their
turn-aware ETAs are within 18%. The export computes ETAs with actual motion
limits; no record size, signature, sentinel or fingerprint changes.

Feast=10 carries the eater's generation and gulp duration. It gives Happy for
45 ticks, forces a Heart bubble within the existing cap, and resolves
one nearest committed denial within 13 radii. Forced Heart/Anger replaces its
owner's bubble or evicts the oldest global bubble when all three slots are full,
matching the prototype; ordinary emissions still drop when full. Each gulp
records start tick, duration `round(30 * clamp(length/26, 1, 3.6))`, origin 0 and strength .35.
Head-on wins also gulp; ties and body kills do not. Two slots replace the oldest
active gulp when full. Widening is visual, never collision geometry; overlapping
gulps use the larger envelope and remain at most +35%. Disabling Power-ups
removes prizes and clears presentation gulps; subsequent V2 head-on wins still
gulp as ordinary combat presentation.

The only checked-in golden mechanics trace is Classic and remains unchanged.
V2 startup/spawn RNG changes deliberately; no existing V2 golden trace was
regenerated or silently relabelled. See the S2 performance report for checks.

## S3 (0.13.0): Venom, ABI still 3

Venom joins Surge/Magnet/Phase; their weights 25/30/20/15 are normalized over
these four enabled kinds (equivalent to section 5's 20/24/16/12). Previous-kind
exclusion remains in effect. Power-ups off clears capsules and held Venom;
already detached nutrition still releases. Classic takes no additional draws.

The shared `world::venom::bite_eligible` rule accepts only rival indices >3 and
strictly past half length, with active Venom, no Phase and no bite immunity.
Movement and feeding precede capsule pickups; wall/head decisions precede
body bites. A successful bite consumes Venom, retains indices [0,cut), and
invalidates removed entries in the tick's existing body buckets. The victim
receives 60 immunity ticks, 48 stump ticks, 78 anger ticks and a 150-tick grudge.
The biter receives 45 Happy ticks with the usual 27-tick blep; only prism meals
emit Heart bubbles, while the bitten victim emits Anger. STRIKE enters within
five own radii of an eligible rear segment and holds through six radii. Losing
Venom or an eligible rear segment clears the pose immediately.

Sever's `snake_id/generation` identify the victim; `other_snake_id/other_generation`
identify the biter. `cut_index` is the first removed segment, `duration_ticks`
is 33, `value` is detached segment nutrition, and `release_tick = tick + 33`.
These event durations never depend on the victim's later length. One reserved
800-point nutrition buffer per victim survives death/respawn and scaling until
release. It is separate from edible/vacuumable food. Free shard food receives
exact detached nutrition distributed over the available particles; ordinary
food-cap and protected-prize/claim eviction rules apply.

Renderer history reserves full prior trails plus two 800-point orphan buffers.
Compact Qt tail history cannot overwrite the full trails. Orphans preserve
original taper, unwrap at seams and sample a precomputed sine table; each is
limited to 100 edges (200 visible edges including seam copies globally).
Cut-glow sprites share that 1200-vertex orphan budget. Calm suppresses wriggle
and holds the tail through tick 33 to match food release, then shortens dissolve.
Shader kinds 23/24 reuse the ordinary head geometry/packing for venom/strike,
leaving mood/pupil/jaw bits intact; bites use the existing kind-6 impact shape.
Stump light combines the existing acid wave-origin bits with kind-25 glow
sprites; Sever runs an acid wave down the biter. No C layout changes.

AI rear-quarter selection measures the 40r search from the bite point rather
than the remote head, so long giants remain reachable. Holders approach along
the rear trail and orbit when confronted within 8r. Defenders face the threat,
with ordinary length-certified head-on safety; coils tighten. Candidate bites
consume the forecast charge and remove detached static geometry; immunity and
Phase use the same rule as mechanics. Front-half, wall, head, self and deposited
neck hazards retain their normal safety checks. Standoffs are intentional turns
and are exempt from anti-circling recovery. Target slot and generation reset
standoff state. Forecasts remain conservative about rival motion and marginal
contacts; only a physical contact consumes the predicted charge.

The S3 chaos fixture carries the reserved 24 Meteor records and fourth-slot
Vortex item alongside live S3 features. Specialized meteor streaks and vortex
rendering remain deferred to S6/S7; that record must stay inert in S3.

## Long-body mechanics (stages A/B)

The config remains 80 bytes. `SnakeLengthLimit` defaults false in Settings;
bit 29 opts V2 into a physical length allowance of 1.5 times arena height,
`floor(height * 1.5 / (radius * 1.18) + 1)` segments. Enabling it stops further
growth of an already longer snake; it does not delete its existing body.
Classic ignores this flag and keeps the released pool, length and speed rules.

V2 has no individual gameplay cap when bit 29 is clear. Its storage ceiling is
6000 segments per snake, and the shared area allowance uses 85% with a ceiling
of 6000 segments. A snake below 80 segments can still grow when that pool is
full. Those per-snake floors and respawns may temporarily exceed the shared
allowance; it is an admission budget, not a destructive hard truncation.
The V2 length speed penalty is bounded at 2.5 (40% of unpenalised speed);
nutrition, boost and effects apply as before. Forecasts use the same rules.

Trails retain exact tick samples through 1600 segments. Above that, history is
compacted once and sampled at half body spacing, retaining the exact live head
and cumulative travel coordinates. Maximum chord error is bounded by half
spacing; ordinary curvature makes it much smaller (about spacing squared over
8 times turning radius). Every physical segment and sweep remains in collision
queries. No body sampling or collision LOD is introduced. Rings retain the
legacy sizing bound with a minimum of 2*(6000+6)+16 samples. Renderer taper
and conservative AI width caches are binned only above 1600. All cut/bulge
indices fit u16; detached and orphan buffers scale with the internal maximum.

### Aggression config bits

The existing config word at byte offset 76 uses bits 16..22 for V2 aggression.
Zero selects the default (100); 1..101 encode 0..100 plus one. Values 102..127
are rejected. Default 100 canonically encodes as zero. Bits 23..28 and 0..15
remain spare and must be zero; length-limit/world-events/power-up bits 29..31
are unchanged. The config is still 80 bytes, ABI version 3. Classic accepts
the field but ignores it; no World RNG draws or mechanics depend on it.

## S4 (0.16.0): Frost, ABI still 3

Frost joins the enabled capsule pool. Weights are Surge 25, Magnet 30, Phase
15, Venom 20 and Frost 10, normalized over those enabled kinds with the existing
previous-kind exclusion. Classic consumes no new RNG draws.

Freezing is independent of the held effect: `frozen_ticks` and FROZEN report
75 ticks after a Nova, while `effect_kind/effect_ticks` still report the rival's
Surge, Magnet, Phase or Venom. Phase does not block cold; its intangibility and
Venom bite rules still apply. Speed is multiplied by 0.5 and the ordinary turn
limit by 0.6. A frozen snake cannot start a burst; Nova immediately cancels an
active burst and its unpaid segment debt. Thaw does not resume that burst.
The holder's Frost effect is consumed immediately, replacing its previous
held effect through the ordinary capsule pickup rule.

Nova=3 carries its owner's identity, `duration_ticks=21` (0.7 s) and `value`
as the world-space radius, exactly 16 config base radii. Its centre is the
holder's pickup endpoint. Alive rival heads within the inclusive radius freeze
unless already frozen or thaw-immune; wrap uses the ordinary world geometry.
All forecasts and tactical valuations share that eligibility primitive.
Movement and feeding precede pickups/Nova, and collisions follow them. Frozen
motion therefore starts on the following movement, while the burst is already
cancelled at the pickup endpoint. Forecasts retain held effects, cancel burst
payments, update all affected rival paths and respect capsule order/guards.

Thaw grants `thaw_immunity_ticks=45`, `scared_ticks=36` internally and a crack
using EffectExpiry=7 with Frost payload `other_snake_id=5` and
`duration_ticks=15`. `breath_ticks` follows the frozen countdown. Power-ups off
clears freezing, immunity and breath along with capsules and held effects.

Shader kind 18 is the Nova quad. Body `params[1]` bit 7 is FROZEN; ice tint is
60% toward #c8eeff, with the established Mono/Pastel accent transform. Crystals
use the existing body ribbon and cost no vertices. The thaw crack reuses kind 6
with `params[2]=1` for seven short sparks. Nova/crack duration comes from the
event, including in Calm; Calm freezes procedural shiver, breath and twinkle.
Both vertex formats retain their layouts, and frozen snakes still interpolate
movement. No record, function signature, ABI version or fingerprint changes.

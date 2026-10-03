# Slithering Snakes: aggression, new look and power-ups

Design spec for the Snakes visual. The prototype `snakes-design.html` shows every element in motion. It has a Current/New toggle, all six palettes, the four backgrounds and the clock overlay. Scenes: Ecosystem, Hunt, Encircle, Power-ups, Anatomy. Keys: 1 to 5 pick the scene, L toggles the look, C the clock, N the notes, Space pauses. The prototype is a Canvas 2D mockup. Its sizes, colours, timings and layering are the spec, but its drawing technique is not. Production rendering is described in section 7.

All decisions below are final recommendations, not options.

---

## 1. Principles

1. **Performance first.** The new look must cost no more CPU than today's renderer. It should cost less, because geometry shrinks about 4x (section 7). Every element has a stated budget.
2. **What you see is what kills you.** Visual body width, taper and head size match collision geometry. There is no decorative wiggle that differs from the simulated path.
3. **Ambient first.** No full-screen flashes and no camera shake. Effects are bounded in time and in size relative to the snake. Peak brightness is capped so the clock stays readable.
4. **Shape before colour.** Power-ups, food types and states are told apart by shape, because colour shifts across the six palettes. Mono must work.
5. **Show intent.** Eyes show where the snake wants to go and what it is doing (calm, hunting, trapped). Boost, kills and pickups send light along the body. This is what makes the AI read as smart and aggressive.

---

## 2. Snake visual language

### 2.1 Body

- **Tube shading.** Three tone bands across the body: an edge at 55% darkened, a mid tone at 22% darkened, and the core at palette colour. The core is offset toward a fixed screen-space light (up-left, direction -0.55, -0.83). A narrow sheen line (palette colour mixed 62% toward white, 55% alpha) runs along the lit side. Bodies read as round and glossy, not as flat ribbons.
- **Contact shadow.** A soft dark fringe at 1.14x the radius (alpha 0.62) separates overlapping bodies. It replaces today's hard 1.275x outline.
- **Outer glow.** Additive, in palette colour, in two falloff steps reaching about 1.85r to 2.15r depending on tier. Base glow alpha is 0.09 to 0.13, and boost, kill and leader add to it. Glow is the main thing that keeps dark palette entries visible on black.
- **Dark palette lift.** Palette entries with relative luminance below 0.30 are mixed toward white by (0.30 - L) x 1.1 for the mid tone. This affects ember `#9c1c28` and the darker forest greens.
- **Taper.** Half-width along the body (u = 0 at the head, 1 at the tail):
  - neck: 0.84 at u = 0, rising to 1.0 at u = 0.07
  - body: 1.0
  - tail: from u = 0.6, `1 - 0.78 * ((u-0.6)/0.4)^1.6`, ending at 22% at the tip, with a pointed tip
- **Collision taper.** The simulation uses the same taper for body collision radius. That is one multiply per segment test, and it is deterministic. It ships together with the visual taper.
- **Breathing.** Width varies ±2.8% as a slow wave travelling head to tail (2.4 rad/s, 0.32 rad per segment). It is visual only and stays within the collision margin. Reduced motion turns it off.
- **Pattern anchoring.** Markings are anchored to the segment index counted from the tail. When a snake grows, the simulation inserts the new segment behind the head, so tail-anchored patterns never jump.

### 2.2 Size tiers

| Tier | Length | Markings | Glow |
|---|---|---|---|
| Hatchling | < 24 | none; bright, clean tube; head 1.24r, eyes 0.36 | 1.75r, alpha 0.11 |
| Adult | 24-99 | dark chevrons pointing at the head, one per segment | 1.85r, alpha 0.09 |
| Elder | 100-249 | chevrons plus dark saddle bands every 6 segments | 2.0r, alpha 0.11 |
| Titan | 250+ | chevrons, saddles, and luminous spine dots every 3 segments with a slow light ripple (2.2 rad/s) | 2.15r, alpha 0.13 |

Tiers use only length. They need no extra simulation state.

### 2.3 Head

- **Shape.** A spade, 1.14r wide (1.24r for hatchlings), reaching 1.42r ahead of the head point and widening past the neck. It is drawn in the same three tone bands as the body, so the neck has no seam. It has two small nostrils and a sheen on the lit side.
- **Eyes.** Placed on the upper sides at (0.50, ±0.56) in head units, radius 0.30 (0.36 for hatchlings). Each has a dark socket ring. Irises are amber `#ffc454`, mixed 22% toward the body colour (silver in mono). There is a small catch-light toward the light.
- **Pupils.** Pupils are slits across the eye, so they read as snake eyes from above. They shift toward the snake's *desired* heading, so you can see it decide where to go before it turns. This uses `desired_angle`, which the frame already exports.

| State | Pupil | Iris | Extra |
|---|---|---|---|
| Calm | medium slit | amber | none |
| Hunting | hairline slit | hot orange `#ff9632` (pale gold on ember, white on mono) | eye glow, alpha 0.35 |
| Trapped | pinprick | near-white | pupils jitter; body glow flickers at 12 rad/s |
| Leader | slit | crown gold | soft gold eye glow |

- **Blink.** 0.15 s every 4-11 s per snake, from a deterministic per-snake hash.
- **Tongue.** A forked tongue extends 1.3r beyond the snout for 0.34 s. It flicks every 3-8 s, or every 1.4-6 s while hunting. Colour is `#ff5c7a` (light grey on mono).
- **Leader crown.** The longest live snake with at least 30 segments wears a gold crown band behind its eyes. The band has three backward spikes and a white-gold gem. The crown is white-hot on ember and mono, because their palettes already contain gold.
  - **Hysteresis:** a rival takes the crown only when it is 3 or more segments longer. This stops today's tie flicker, where two equal snakes both get crowns.
  - **Succession:** the new leader gets a gold ring (0.6 s), and a light pulse runs down its body every 4 s.
  - The crown replaces today's large crown glyph, which hides the eyes.

### 2.4 Light waves (one mechanism, many uses)

A wave is a bright band about 7 segments wide that runs head to tail. Each snake has at most 2 active waves.

| Trigger | Colour | Speed (body lengths per second) | Strength |
|---|---|---|---|
| Boost | white-tinted body colour | 2.4, one wave every 0.11 s during the burst | 0.55 |
| Kill (the killer) | near-white | 1.1 | 1.0, plus eye flare |
| Power-up pickup | power-up accent | 1.3 | 0.95 |
| Leader | crown gold | 0.75, every 4 s | 0.5 |

### 2.5 Boost visuals

Boost visuals appear only during a burst.

- **Bow wave.** Two curved light strokes start just ahead of the snout and sweep back past the head.
- **Contrail.** A fading additive streak behind the tail tip. It is 0.5 s long and about 2.4x the tail width.
- **Body.** Glow alpha rises by 0.14, and boost waves run down the body.
- **Start ring.** 1r growing to 3.2r over 0.28 s.
- **Spent pellets.** The segments paid for the boost drop from the tail as small dim pellets. You can see the cost and follow the trail.

### 2.6 Death

1. **Impact.** At the contact point: a white flash (radius up to 7r), a ring growing from 1r to 6r, and 9 short radial sparks. All are tinted with the victim's colour and last 0.5 s.
2. **Corpse.** The body flashes white for about 0.08 s, then breaks into 3-segment pieces that drift apart and fade. The pieces go head to tail over 0.55 s.
3. **Essence shards.** These are the death food. Today's mechanics are unchanged (count, value, velocity), and the shards scatter while the corpse dissolves.
4. **Kill glow.** If the death was caused by a rival's body or by losing a head-on, that rival gets a kill wave and an eye flare.

---

## 3. Food

All food is one sprite. Kind is shown by shape.

| Kind | Source | Look | Value / life |
|---|---|---|---|
| Spark | ambient (today's food) | round core with a hot white centre and soft halo (4.6x size); rare 4-point twinkle | unchanged |
| Essence shard | death bursts | slowly spinning diamond in the victim's colour, with a flickering halo | unchanged (death mechanics) |
| Spent pellet | boost cost | tiny dim dot with a faint halo | 0.5 / 8 s |
| Prism fruit | rare spawn | larger glossy orb with a tilted orbiting ring | 5.0 / 30 s |

- **Prism fruit spawning.** One at a time, every 40-60 s, at the item spawn location rule (section 5.1). It is the prize that makes snakes race and boost.
- **Expiry.** In the last 3 s of life, food fades out with a flicker. This needs the life fraction exported.
- **Vacuum streak.** When a snake pulls food in, the streak is a gradient tail from the food away from the head. Today's mechanic is unchanged.
- **Breathing.** Food pulse and twinkle are computed in the shader from `phase` and time. The CPU no longer computes a per-food `sin`.

---

## 4. Boost and aggression rules

### 4.1 Current state (found in `world.rs` and `ai/mod.rs`)

`rush` is free today. The attack burst uses `rush = traits.aggression`, which is anywhere in [0, 1], meaning up to +100% speed. There is also a 0.25 dodge rush and a +20% digestion boost. Free boost removes any trade-off and makes attacks hard to read.

### 4.2 New boost rule (all snakes)

| Parameter | Value |
|---|---|
| Burst | 24 ticks (0.8 s) at x1.6 speed (rush 0.6). The turn rate keeps today's speed/min-turn-radius limit, so boosted turns are wider. |
| Cost | 2 segments, plus 1 per full 100 segments of length. Removed from the tail evenly over the first 21 ticks. Each removed segment becomes a spent pellet (value 0.5, life 8 s). |
| Minimum length | 12 segments |
| Cooldown | 36 ticks (1.2 s). Under Surge: free, with an 18-tick cooldown. |
| Frozen | cannot boost; an active burst is cut to cooldown |
| Wind-up | none. Aggression has priority, and rivals see the boost flag from the first tick. |
| Removed | the free variable rush and the 0.25 dodge rush |
| Kept | the +20% digestion boost |

Long snakes pay more segments, but a smaller fraction of their length. That lets big snakes afford traps while small snakes must choose their bursts. Pellets return a quarter of the cost to the field, which leaves a visible trail that others can follow.

**Determinism.** Boost is a per-snake tick counter driven by the steering input (`rush > 0` requests a boost). It needs no new RNG draws.

### 4.3 AI use of boost

- **Attack.** The existing staged cut-off (approach, burst, crossing) uses the fixed 0.6 burst. It may start only when boost is ready and length is at least 16. The planner budgets the segment cost into its score, so a cut-off must be worth more than about 2.5 food.
- **Escape.** Boost when the safe horizon is under 0.6 s and a boosted rollout finds a safe path that an unboosted one doesn't. This is allowed at length 12 or more.
- **Food race.** Boost when a rival is within 25% of the same distance to a target worth 3 or more (death fields, prism fruit).
- **Never** boost into a turn tighter than the boosted minimum turn radius.

### 4.4 Encirclement

- Long snakes (at least 3x the victim's length and 150 or more segments) attempt coils against walls and in open space. The README notes that natural runs admit zero pockets today. Admission is relaxed to "victim within 20 radii, coil radius at least 4.2 own radii, enough body for 1.2 loops."
- The simulation sets **TRAPPED** on a snake when its reachable area (already computed, `reachable_cells`) drops below 1.5x what it needs to turn around. It sets **HUNTING** on a snake that has a prey or an active coil. The renderer reads only these flags.
- The head-on rule is unchanged: a snake 4 or more segments longer wins, otherwise both die. Hitting a body kills the striker.

---

## 5. Power-ups

### 5.1 Item system

- **Look.** A dark hexagon capsule (radius 2.1 base radii) with an accent-coloured rim that turns slowly, an inner hexagon line, the icon, an orbiting spark and a soft halo. The capsule shape tells items apart from food at a glance.
- **Accents:** Surge `#ffe14d`, Magnet `#ff5fd2`, Phase `#a98bff`, Venom `#9dff3a`, Frost `#bdf3ff`. On mono they are desaturated 88% toward grey. On pastel they are lightened 30%.
- **Spawn.** The field holds `clamp(round(snakes/4), 1, 3)` items. A world-RNG timer of 15-30 s triggers each spawn. Twelve candidate points are sampled, and the one with the most clearance from any segment wins. It must be at least 10 base radii from deadly walls and 20 from other items. The same kind never spawns twice in a row.
- **Weights.** Magnet 30, Surge 25, Venom 20, Phase 15, Frost 10.
- **Life.** Items appear with a ring that closes inward (0.5 s). They live 25 s and blink faster over the last 3 s. When they expire they leave a small collapsing ring.
- **Pickup.** The head comes within 1.3r plus the item radius. Activation is instant, with no holding. Pickup shows an accent ring burst from the item, a white inner ring and an accent wave down the snake.
- **One effect at a time.** A new pickup replaces the current effect. In the last 1.2 s, an accent ring blinks around the head as an expiry warning. Expiry shows a small ring closing inward.
- **Setting.** A "Power-ups" checkbox in Settings, on by default.

### 5.2 The five power-ups

| | Effect | Duration | Active look | Counter-play | AI value / use |
|---|---|---|---|---|---|
| **Surge** (bolt) | Boosts cost nothing; cooldown 0.6 s; base speed +25% | 180 ticks (6 s) | Bright crackling arcs along the rim; boosts nearly back to back | Rivals treat a surged snake's reach as 1.6x and widen their margins | 6 base, +6 if a prey is within 25r. Chains cut-offs; the most visibly aggressive item. |
| **Magnet** (horseshoe) | Food pull reach 3r becomes 9r | 300 ticks (10 s) | Dashed accent ring at 9r turning around the head with 3 orbiting sparks; long vacuum streaks | None needed (non-lethal); others race for death fields | 4 base, + local food density. Goes for death fields and prism fruit: "the scavenger". |
| **Phase** (ghost diamonds) | Passes through all bodies, its own included. Its body does not kill others. Head-on collisions are ignored. Walls still count. | 120 ticks (4 s) | Body at about 45% opacity with flicker, dashed accent outline moving along it, hologram scan bands | If Phase ends while the head is inside a body, normal rules apply and the snake dies. Rivals ignore a phased snake as a threat. | 3 base, 15 when TRAPPED. Escapes coils at the last moment. AI plans so its head is clear at expiry. |
| **Venom** (fangs) | Head bites through a rival's back half (segment index above half the length): the rival is cut there, and the cut piece bursts into shards. The rival survives, shorter. Hitting the front half kills the biter as usual. Ends after one bite. | 240 ticks (8 s) or one bite | Acid tint on the head, small fangs, acid spine line, falling drip sparks; the bite is a green impact | Rivals keep their tails tucked or coiled; venom snakes are treated as a tail threat | 5 base, +5 if a snake 1.5x longer is within 30r. Targets the tails of giants, the only way a small snake hurts a titan. |
| **Frost** (snowflake) | Instant nova at pickup. Rivals whose heads are within 16 base radii get speed x0.5 and turn rate x0.6, cannot boost, and an active burst is cut. | nova 0.7 s; frozen 75 ticks (2.5 s) | Expanding ice ring with crystal spikes. Frozen snakes turn icy (60% mix to `#c8eeff`) with frost sparkles. | Stay away from a Frost item when a rival is closer to it | 3 base, +8 per rival within 16 base radii of the item at arrival. Combos: frost, then a boosted cut-off on the slowed prey. |

All effects are tick counters plus radius checks on the existing spatial grid. They are deterministic, with no new RNG use except at spawn.

---

## 6. Readability rules (ambient and clock)

- Peak additive brightness from any one effect: alpha 0.9 for at most 0.5 s, within 7 snake radii. Nova is the largest effect at 16 base radii, with a faint fill.
- At most 8 short-lived effect quads alive. Further effects are dropped, oldest first.
- **Clock scrim (decided: on).** When the clock is shown, a soft radial darkening sits under it. It is 2x the clock block wide and 3.2x as tall, alpha 0.52 at the centre fading to 0. Bright bodies and effects that pass under the clock no longer wash out the text. It is a single static image item in QML and moves with the clock.
- **Reduced motion.** Freezes the shader time uniform, so there is no breathing, ripple, twinkle or crackle flicker. Blinks, waves and death effects still play, shortened to 60%.
- Effects use the same wrap and seam logic as food, so they cross monitor borders correctly in seamless mode.

---

## 7. Rendering approach and cost

### 7.1 Today (measured from `src/snakerenderer.cpp`)

- One `QSGGeometryNode` with `QSGVertexColorMaterial` and `ColoredPoint2D` (12 B per vertex), drawn as one draw call.
- **Body:** outline ribbon plus fill ribbon is 12 vertices per edge. Markings add about 3 per segment. That is about **15 vertices per segment**.
- **Per snake:** tail outline disc 36, tail fill disc 36, head disc 36, eyes 90. That is about **198 vertices**, plus 81 for the crown.
- **Food:** 63 vertices each (48 when dense), plus 6 for the vacuum streak.
- **Mature world** (about 1100 segments, 210 food): about **32k vertices, 380 KB uploaded per frame**. This matches PERF_REPORT's 31.7k.

### 7.2 New: one custom material, still one draw call

- **`SnakeMaterial`.** A `QSGMaterial` with a precompiled `.qsb` vertex and fragment shader pair built through `qt_add_shaders`. It adds a build dependency on `qt6-shadertools-dev` and no runtime dependency.
- **Vertex layout, 24 B:**
  - `pos` (2 floats)
  - `uv` (2 floats): across in [-1, 1] and along in segment units, anchored at the tail
  - `color` (4 unsigned bytes)
  - `params` (4 unsigned bytes): primitive kind, tier or effect, flags, wave brightness
- **Uniforms:** matrix, opacity, time (simulation time plus interpolation, so all monitors agree), and the light direction.
- **One texture:** a 128x128 R8 signed-distance atlas holding the 5 power-up icons (16 KB).
- **Blending:** premultiplied "over", which is Qt's default. Glow pixels write alpha 0, which makes them additive in the same pass. Opaque, shadow and glow layers therefore need no extra draw call or blend-state switch.
- **Fallback.** If the scene graph backend is Software, or the shader fails to load, the current renderer runs unchanged as "classic". New gameplay (boost, power-ups) still works there, with the R1 vertex-colour visuals.

### 7.3 Per-element budget

| Element | Vertices | Extra draw calls | Fragment work | CPU |
|---|---|---|---|---|
| Body (tube bands, shadow, glow, markings, sheen, taper, breathing, tier patterns, titan spine lights, effect looks: Surge arcs, Phase scan bands and dashed outline, Venom spine, Frost tint) | **6 per edge** (was 15 per segment), one ribbon 2.15r wide | 0 | about 30 ALU; one branch on the effect byte | Same ribbon walk as today. Taper and wave brightness are about 2 multiply-adds per point. |
| Head (head shape, eyes, pupil states, blink, nostrils, crown, venom fangs, tongue, boost bow wave) | **6** (one quad, 3.2r x 3.0r, stretched forward 1.5x while boosting) | 0 | about 60 ALU signed-distance shape, head pixels only | Pupil offset and state flags packed into params |
| Light waves (boost, kill, pickup, leader) | 0 (per-vertex brightness byte) | 0 | 1 multiply-add | At most 2 waves x points walked |
| Boost contrail | at most **84** per boosting snake (14 edges from a 15-entry tail ring buffer) | 0 | streak gradient | Ring buffer per snake, held by the renderer |
| Food: spark, shard, pellet, prism, expiry fade | **6** (was 63) | 0 | about 20 ALU; pulse and twinkle move from CPU to shader | Removes one `sin` per food per frame |
| Vacuum streak | 6 (unchanged) | 0 | gradient | unchanged |
| Power-up item | **6** each, at most 3 on the field | 0 | hexagon distance field plus one atlas sample | negligible |
| Magnet ring | 6 | 0 | ring distance field | negligible |
| Effect quads (spawn, pickup, expiry rings, impact with 9 sparks, nova, succession ring) | **6** each, capped at 8 alive (at most 48) | 0 | ring and spark distance fields | Hash-seeded, with no per-frame RNG |
| Effect particles (venom drips, frost stars) | 6 each, capped at 24 (at most 144) | 0 | sprite | negligible |
| Corpse dissolve | 6 per edge for 0.55 s, one corpse at a time typically | 0 | dissolve threshold on `uv.along` | none (the simulation exports corpse segments) |
| Clock scrim | QML `Image` (pre-rendered 256 px radial PNG, scaled) | **+1**, static, only when the clock is on | one textured quad | 0 |

### 7.4 Mature-world estimate

| | Today | New |
|---|---|---|
| Vertices | 1100 x 15 + 10 x 198 + 81 + 210 x 63 = about **31.8k** | 1100 x 6 + 10 x 6 + 210 x 6 + extras = about **8k** (peak about 8.6k with effects) |
| Upload per frame | about **380 KB** | about **190-205 KB** |
| Fill | baseline | about equal: body coverage per segment goes from 4.47r to 4.30r of width; glow replaces the outline ribbon |
| Draw calls | 1 | 1 (plus 1 for the clock scrim) |
| Full-screen passes | none | none |

Expected renderer CPU: **-40% to -60%** (vertex generation and copy dominate). Expected GPU: about the same. This must be measured, not assumed (section 9).

### 7.5 Simulation and ABI cost

- **ABI v2.** `SNAKES_CORE_ABI_VERSION` becomes 2, and the static asserts are updated.
  - **Snake record:** `flags` (BOOSTING, COOLDOWN, HUNTING, TRAPPED, FROZEN, PHASED, LEADER, CORPSE), `effect_kind` (u8), `effect_ticks` (u16), `boost_ticks` (u8).
  - **Food record:** `kind` (u8) and `life_fraction` (u8).
  - **New arrays:** power-up items (id, x, y, kind, age_ticks, life_ticks), and an event ring of up to 32 entries per frame (kill, sever, pickup, nova, succession) with tick, position, the two snake ids and colour.
  - **Corpses.** A dead snake keeps exporting its last segments for 17 ticks with the CORPSE flag, then clears. The renderer needs no history lookup.
  - **Leader.** The leader with its hysteresis is computed in the simulation and exported as the LEADER flag, so every monitor agrees.
- **Tick cost.** Boost counters, effect counters, the item timer, the pickup check (heads x at most 3 items), nova (one radius pass over heads), sever (a branch inside the existing body-hit path), and the Phase skip (a branch). Estimated **under 0.03 ms per tick**. The AI additions (item values in target scoring, the boost budget, the Phase-expiry check) are about 0.02-0.05 ms.
- **Determinism.** All new randomness uses the world RNG at spawn time only. The golden and parity traces need regeneration once per release that changes mechanics.

---

## 8. Build order (release slices)

Each slice ships on its own and must pass the performance gates in section 9.

### R1 (0.8.0): Boost and aggression

Mechanics and AI, with minimal visuals in the existing vertex-colour renderer.

1. **Simulation:** the boost rule (4.2) replaces free rush; spent pellets; ABI v2 fields for flags, boost and food kind; corpse retention; the event ring; leader hysteresis.
2. **AI:** boost budgeting for attack, escape and food races (4.3); relaxed coil admission and the TRAPPED/HUNTING flags (4.4). Re-run the scorecard and duel suites. Target: exact kills at least equal to today, and self deaths no worse than today.
3. **Classic-renderer visuals**, all within today's material:
   - Burst: fill colour lerps 35% toward white (0 extra vertices).
   - Contrail: up to 84 vertices per boosting snake.
   - Pellets: smaller and dimmer food, 4-sided discs.
   - Impact flash: 2 discs, 12 sides, 72 vertices for 0.5 s.
   - Hunting: amber eye white with a smaller pupil.
   - Trapped: pinprick pupil.
   - Crown hysteresis.

### R2 (0.9.0): The new look

1. `SnakeMaterial` and its shaders: body tube, glow, shadow, taper, tiers and markings, waves, head with eyes, blink, tongue and crown, food sprites, contrail, bow wave, impact quad, corpse dissolve. Classic stays as the fallback.
2. Collision taper in the simulation, so visual and collision geometry match.
3. Clock scrim in `Screensaver.qml`.
4. Reduced-motion handling.
5. Update `tests` geometry fingerprints, and add a vertex-count regression test asserting at most 6 vertices per body edge and per food.

### R3 (0.10.0): Power-ups, first set

1. Item system (5.1), Settings checkbox, items in ABI v2, atlas texture.
2. Magnet, Surge and Phase, with their AI values and counter-play.

### R4 (0.11.0): Power-ups, second set

1. Venom (severing), Frost (nova and frozen state), and prism fruit.

Nothing is left as stretch. Tongue flicks, blink and tier patterns are free in the head and body shaders, so they ship in R2.

---

## 9. Performance gates (every slice)

- **CPU.** Process CPU at or below today's 3-8% of one core, on the user's three-monitor seamless setup (2560x1440, 3440x1440 and 1920x1080 at 100, 175 and 240 Hz). Measure with `scripts/frame-timing.sh` and `scripts/benchmark-snakes.sh`.
- **Frame pacing.** Frames land within 1 ms of target at every refresh.
- **Renderer.** `test-snakerenderer benchmarkMatureGeometry`: R2 at most 0.6x of today's sync time. R1 and R3/R4 at most 1.05x of the previous release.
- **Simulation.** Mean tick increase at most 0.05 ms, and still zero steady-state allocations (the existing allocation test, extended to items, events and pellets).
- **Rollback.** If R2 misses the CPU gate, keep classic as the default and ship the shader look as an option until fixed. Do not ship a regression.

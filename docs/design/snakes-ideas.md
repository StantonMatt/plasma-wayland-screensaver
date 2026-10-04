# Slithering Snakes: the fun pass

Design proposal for what comes after 0.10.0. It builds on `docs/design/snakes-design.md` (called "the spec" below) and keeps all of its principles: performance first, what you see is what kills you, ambient first, shape before colour, show intent.

The prototype `ideas.html` shows every shortlisted idea in motion. Scenes: Faces, Race, Venom, Frost, Prism, Flip, Whirlpool, Events, Chaos. Keys: 1 to 9 pick the scene, C toggles the clock, M toggles Calm (reduced motion), N the notes, Space pauses. The URL hash deep-links a state, for example `#scene=venom&palette=ember&clock=1&calm=1&t=4`. The prototype is Canvas 2D: its sizes, colours, timings and rules are the design, its drawing technique is not. Section 5 maps each element onto the real single-material renderer.

All decisions below are recommendations, not options.

---

## 1. What makes it fun to watch for minutes

Three things carry the fun. The shortlist is chosen for them.

1. **Characters, not sprites.** Snakes that grin after a kill, glare when an item is stolen from under their nose, yawn at night and go dizzy after a U-turn. Faces turn every AI decision into a small story. They cost almost nothing, because the head is already a signed-distance shader.
2. **Visible races and plans.** You should see who is going for what, who is winning, and who gave up. Then the payoff lands: the cut-off, the denial, the grudge.
3. **Set pieces with a build-up.** Things you can see coming (a seed ripening, a vortex charging, a landing zone shimmering) make snakes converge, and the convergence makes the drama.

Everything else (flashes, screen-wide effects, more particles) was rejected or capped.

---

## 2. Brainstorm (16 ideas)

| # | Idea | One-line pitch | Verdict |
|---|---|---|---|
| 1 | **Faces** (moods and emote bubbles) | 9 moods in the eyes; rare "!", "?", anger, Zz and heart bubbles | **Shortlist #1** |
| 2 | **Race for the capsule** | Drop-in target, contest arcs, denial, grudges, hoarding | **Shortlist #2** |
| 3 | **Prism fruit + Gulp** | A seed ripens, snakes circle like vultures, the winner swallows with a visible bulge | **Shortlist #3** |
| 4 | **Venom** (refined) | Bite the giant's tail; the tail wriggles on its own like a lizard's | **Shortlist #4** |
| 5 | **Flip** (new item) | The one item a snake holds: false eyes on the tail, then an instant U-turn | **Shortlist #5** |
| 6 | **Frost** (refined) | Nova freezes rivals: icy skin, shivering, frosty breath, then a cut-off | **Shortlist #6** |
| 7 | **Starfall and Nightfall** (world events) | Meteor shower of gold food; a slow dusk where only the eyes shine | **Shortlist #7** |
| 8 | **Whirlpool** (new item) | Pickup opens a vortex that brews a feast, then bursts into a ring | **Shortlist #8** |
| 9 | Rampage streaks | 3 kills in 20 s: tally pips behind the head, embers on the body, rivals flee | Backlog 1: cheap, but kills are already rewarded by Faces |
| 10 | Hatching and molting | Respawns hatch from an egg; a tier-up sheds a ghost skin | Backlog 2: charming, low risk, not a gameplay change |
| 11 | Seasonal touches | Halloween pumpkin prism and jack-o'-lantern eye glow, winter snowflake food and hat crown | Backlog 3: delightful for a few weeks a year; date-driven, opt-out |
| 12 | Mitosis (split into two) | Tail half becomes a temporary clone | Rejected: needs a free snake slot, a second AI brain (AI is the main CPU cost) and ABI identity churn |
| 13 | Portal pair | Enter one ring, exit the other | Rejected for now: breaks body continuity for collisions, forecasts and the ribbon; huge test surface |
| 14 | Ghost decoys | Phantom copies fool rivals | Rejected: hard to tell real from fake at a glance, so it reads as noise |
| 15 | Shrink ray / Mini | A snake shrinks to slip through gaps | Rejected: segment spacing is tied to radius, so length pops visibly |
| 16 | Shockwave, lightning, tornado | Knock snakes around | Rejected: snakes are steered, not physics bodies; knockback breaks forecasts, and flashes are a photosensitivity risk. Its food-only version became Whirlpool |

Also considered and deferred: team packs by colour (pincer cut-offs; the AI is costly and palettes repeat colours unevenly), bubble shield (overlaps Phase), and chili-pepper food (overlaps Surge).

---

## 3. Shortlist, ranked

Ranking is fun to watch per unit of cost and risk. Ticks are the 30 Hz simulation ticks. "r" is the snake's own radius, "base radii" is the config's base radius, as in the spec. Every rule is a tick counter or a radius check, and every random draw uses the world RNG at spawn time only.

### 3.1 Faces: moods and emotes (rank 1)

**What it is.** Each head shows one of nine moods. Rare speech bubbles pop above a head for the big moments.

**Why it's fun.** You can read a snake's life story from across the room: it spots the capsule ("!"), loses it at the last moment (glare and anger bubble), chases the thief with a grudge, wins a head-on and grins with its tongue out. It costs almost nothing and amplifies every other idea.

**Moods** (one 4-bit code per head; highest priority wins):

| Priority | Mood | Trigger (sim, deterministic) | Look (head shader) |
|---|---|---|---|
| 1 | Frozen | FROZEN flag | icy iris, squinting lid, shiver ±0.05 head units at 11 Hz, frosty breath puff ahead of the snout |
| 2 | Dizzy | 30 ticks after a Flip | spiral eyes, slowly turning |
| 3 | Trapped | existing TRAPPED | pinprick pupils with jitter (shipped) |
| 4 | Happy | 45 ticks after a kill, pickup or feast | closed smiling crescents; tongue held out ("blep") for the first 27 ticks |
| 5 | Angry | 78 ticks when denied an item or bitten | slanted lid cutting deeper toward the snout, red-orange iris, thin slit, faint red eye glow |
| 6 | Scared | a HUNTING rival whose prey is me, or a Venom holder targeting me, within 14 r; or 36 ticks after a longer snake kills within 12 r of me | wide white eyes, tiny round pupils with jitter, a sweat drop beside the head |
| 7 | Hunting | existing HUNTING | hairline slit, hot iris (shipped) |
| 8 | Sleepy | Nightfall, or 600 ticks with no target and not hunting | half-closed lids; yawns every 150-360 ticks (per-snake hash); jaw opens for 39 ticks |
| 9 | Calm | default | amber slit (shipped) |

**Emote bubbles.**

| Glyph | Trigger | Tint |
|---|---|---|
| "!" | commits to a big target: item (also during drop-in), prism seed, Starfall zone | item accent, else warm white |
| "?" | abandons a big target it had chased for at least 30 ticks | pale blue-grey |
| Anger (four curved brackets) | denied: the nearest committed loser within 13 r of a pickup or feast; bitten by Venom | red-orange |
| Zz | yawn | pale blue |
| Heart | eats a prism fruit | pink |

- **Rules.** At most one emote per snake per 150 ticks, except Anger and Heart, which replace that snake's own bubble. At most 3 bubbles on screen; a new non-forced bubble is dropped when full. **One reaction per event:** only the nearest loser reacts, so a pickup never sets off a chorus.
- **Placement.** Above-right of the head in screen space (offset 1.9 r, -2.6 r), clamped to the monitor, and never drawn inside the clock area.
- **Life.** 45 ticks: a 0.18 s pop with a small overshoot, a hold, then a 0.3 s fade.
- **Grudge (AI).** An angry snake remembers who wronged it for 150 ticks. If the culprit is no more than 1.2x its length and within 30 r, it becomes preferred prey (+40% hunt score). It reads as "it's personal" and costs one id and one counter.

**Cost.**

| Element | Vertices | Fragment | CPU / sim |
|---|---|---|---|
| Moods, blep, yawn, sweat drop, breath puff | 0 (all inside the existing head quad; sweat drop at head-local (-0.3, ±1.35), inside the 1.75 side bound) | about +20 ALU on head pixels only (one switch) | mood computed in the sim: a few compares per snake per tick, under 0.002 ms |
| Bubble | 6 each, at most 3 (18) | circle and tail SDF plus one atlas sample, about 25 ALU over a 2.6r quad | rides the existing event ring (new kind Emote, glyph in `other_snake_id`) |

**Reduced motion.** Moods stay, because they are static shapes. No bubble overshoot (fade only), no pupil jitter, no spiral spin, no shiver, no sweat slide, no breath puff.

**Risk.** Low. The danger is clutter, which the caps and the one-reaction rule handle. Faces must stay legible at hatchling size: on a 1080p monitor the head is about 22 px, and the prototype's Faces scene shows each mood at that size.

### 3.2 Race for the capsule (rank 2)

**What it is.** The visual language for the racing and fighting AI the engineer is building now.

**Why it's fun.** A spawn becomes an event: heads turn, "!" bubbles pop, someone gives up with a "?", a trailer burns segments on a boost, the leader cuts across its line, and the loser glares and gives chase.

**Rules.**

- **Drop-in.** An item is announced 30 ticks before it can be taken: a closing target ring with four crosshair ticks, a dashed ghost hexagon and a faint icon. It is not collectible and not an obstacle while announced. AI may commit during drop-in, which is what makes the race visible.
- **Commit.** The sim exports `target_item` per snake (u8 slot, 255 for none).
- **Contest arcs.** For each item, the two committed snakes nearest to it each light the side of a ring (radius 1.62 capsule radii) that faces them, in their own colour:
  - closeness `c = clamp(1 - (d - 3r)/(34r), 0, 1)`
  - half-angle 0.22 + 0.95c rad, alpha 0.3 + 0.6c, stroke 1.6 + 2.6c px
  - the leader's arc is drawn brighter
  - **Contested** (ETAs within 18%): both arcs pulse at 1.1 Hz.
- **Denied.** The nearest committed loser within 13 r gets Angry, an Anger bubble and a grudge. Everyone else drops the target quietly.
- **Hoard (AI behaviour).** A snake that already holds an effect with more than 90 ticks left would waste it by picking up a new item. Near a valuable item (value 5 or more, within 15 r), it orbits the capsule at 5-7 r instead and takes it when its own effect is down to 30 ticks or less. Meanwhile it treats approaching rivals as cut-off targets. A dragon on its hoard: it is rule-justified, readable and very funny. It uses the existing coil planner around a fixed point.

**AI hooks for the engineer** (how the race should be decided so that it reads well):

- **ETA.** `distance / speed + |heading error| / turn rate`, using the rival's actual turn-rate limit.
- **Commit** when my ETA < 0.9 x the best rival's ETA, or when my ETA < 1.25 x and boost is ready (length 16 or more).
- **Race boost** when a rival's ETA is within 25%, distance < 18 r and heading error < 0.45 rad. This is spec 4.3; the cost is budgeted.
- **Abort** when the best rival's ETA < 0.8 x mine for 10 consecutive ticks. The hysteresis stops dithering, and the abort emits "?".
- **Deny** when trailing: if the leader's path to the item crosses my reachable set at least 3 r ahead of it, run the existing staged cut-off against the leader instead of racing.

**Cost.**

| Element | Vertices | Fragment | CPU / sim |
|---|---|---|---|
| Drop-in telegraph | 0: the capsule quad in an "incoming" state (birth byte range reused) | ring, ticks and dashed hexagon SDFs in the capsule branch | spawn event moves 30 ticks earlier |
| Contest arcs | 6 each; at most 2 per item x 3 items = 36 | arc SDF, about 15 ALU | renderer computes closeness from exported positions |
| Hoard, grudge | 0 | 0 | AI only; the coil planner already exists |

**Reduced motion.** Arcs do not pulse; the telegraph ticks do not rotate.

**Risk.** Low for the visuals; medium for the AI (owned by the engineer). The drop-in delay shifts item timing, so golden traces must be regenerated once.

### 3.3 Prism fruit and Gulp (rank 3)

**What it is.** The planned prize (spec section 3), now with a build-up and a payoff.

**Why it's fun.** A seed appears and its ring fills like a timer. Everyone in range turns and says "!". The early arrivals circle like vultures. When it ripens they pounce, and the winner swallows it whole: a bulge slides down its body under a rainbow shimmer.

**Rules.**

- **Spawn.** One at a time, every 1200-1800 ticks (40-60 s), using the item spawn-location rule.
- **Seed** (new food kind PrismSeed). Not edible and not an obstacle. Ripens over 90 ticks (3 s), then becomes Prism (value 5, life 900 ticks).
- **Feast.** Eating a Prism emits a Feast event: the eater gets Happy, a Heart bubble, a rainbow wave and a gulp bulge, and Denied is resolved as in 3.2.
- **Gulp bulge.** Also triggered by a head-on win (the winner "swallowed" the loser). Rendered as width x (1 + 0.35 e^(-z^2)), z = (i - c)/2.1. The centre c travels head to tail in max(1 s, length/26 s), capped at 3.6 s. At most 2 bulges per snake.
- **Principle exception.** The bulge is visual only and up to +35% wide. Like breathing, it is a stated exception to "what you see is what kills you": it is transient and inside the body's own glow.

**AI.**

- **Value.** Prism is worth 9; a seed is worth 9, discounted by the time it has left to ripen.
- **Timing.** Plan to arrive at the ripe tick. If early, orbit at 5-6 r on the side facing the approach.
- **Pounce.** At ripe, go in; boost if any rival's ETA is within 25%.
- **Hunters.** Treat the circling crowd as prey-rich: an ambush near a seed is legitimate drama.

**Cost.**

| Element | Vertices | Fragment | CPU / sim |
|---|---|---|---|
| Seed with ripening ring | 0 extra (food sprite kind 8; ripeness in the existing life byte) | progress-arc SDF | one timer |
| Rainbow wave | 0 | 0 (vertex colour) | hue from a 6-entry table for at most about 16 vertices under the wave |
| Bulge | 0 | 0 (the quad widens with the vertex width) | one multiply-add for about 13 vertices per bulge |

**Reduced motion.** The ring still fills (it is information). No orbiting dot, no hue drift in the rainbow; the bulge plays 40% faster.

**Risk.** Low.

### 3.4 Venom, refined (rank 4)

**What it is.** The spec's tail-biting item (section 5.2) with a strike pose, a wriggling severed tail and a standoff.

**Why it's fun.** A small snake stalking a giant's tail is inherently comic. The bite pays off twice: the tail keeps wriggling on its own like a lizard's, then bursts into essence. The giant's counter-play (turning to face the biter) creates a little dance.

**Rules** (spec rules kept; additions marked):

- Lasts 240 ticks or one bite. A head contact with a rival segment whose index is above half its length (and above 3) cuts the rival there. A front-half contact uses the normal rules, so the biter dies.
- **New: strike pose.** The sim sets STRIKE while the holder is within 5 r of a valid bite point. The jaw opens and the fangs lengthen.
- **New: stump.** The victim's new tail tip glows acid for 48 ticks. The victim gets Angry, an Anger bubble and a grudge on the biter.
- **New: bite immunity.** A bitten snake cannot be bitten again for 60 ticks, which prevents chain bites that look like glitches.
- **New: orphan tail (renderer only).** On the Sever event, the renderer copies the cut piece from its last frame into a preallocated orphan buffer (cap 2 orphans) and wriggles it for 33 ticks. The wriggle is a lateral `0.75r (1-p) sin(0.75 i - 17 t)` from a table, growing toward the tip. The piece then dissolves like a corpse. The essence shards come from the sim as before, released when the wriggle ends, so the sim keeps them back for 33 ticks.

**AI.**

- **Holder.** Targets a rival at least 1.3x longer within 40 r. Aims at the rear quarter (0.78 of its length) and approaches from behind along the rival's own trail, so its path never crosses the front half. Aborts if the target's head turns to face it within 8 r (the standoff), then circles to try again.
- **Defender** (counter-play). When a Venom holder within 16 r is aiming at my back half, turn so it sits in my front 90 degrees: show the head. If I am 4 or more segments longer, go head-on, which I win. Long snakes tighten their coil to tuck the tail.
- **Everyone.** A Venom holder's head is a tail threat: keep tails out of a 6 r cone ahead of it.

**Cost.**

| Element | Vertices | Fragment | CPU / sim |
|---|---|---|---|
| Acid head tint, fangs, drool beads, strike jaw | 0 (head quad) | about 12 ALU in the head branch | STRIKE flag |
| Acid spine | 0 | body branch on effect 4: one line SDF | 0 |
| Orphan tail | 6 per edge, for 1.1 s; at most about 1.2k vertices for a titan's half | body shader with the CORPSE flag | a copy of at most MAX_SEGMENTS/2 points, no allocation; about 1 µs |
| Bite impact | 1 effect quad (existing kind 6, venom colour) | existing | branch in the existing body-hit path |

**Reduced motion.** No wriggle (the tail dissolves in place), no drool animation.

**Risk.** Medium: severing changes a length mid-tick, and the orphan buffer must stay allocation-free. This was already planned for R4.

### 3.5 Flip, new item: the held U-turn (rank 5)

**What it is.** Accent `#ff9a3c`, icon two opposed arrows. The only item a snake *holds*: it decides when to use it. On use, head and tail swap in one tick.

**Why it's fun.** It is the purest "clever use" item, which is exactly what the user asked for. A chaser closes in on the tail, the false eyes on the tail tip glint, and suddenly it is nose to nose and loses the head-on. Or a trapped snake reverses out of a closing coil. The new head comes up dizzy, with spiral eyes.

**Rules.**

- Spawn weight 10. While held (up to 300 ticks), the body carries effect kind 6 and the tail tip shows two eye-spots in the accent colour with a soft glow.
- **Activation** is a steering bit from the AI. The sim reverses the segment array in place (O(n) swap, no allocation), calls the existing `rebuild_trail(i)`, and sets the heading from the new segment 1 to the new head. Boost state is kept and the effect is cleared. With self-collisions on, the new head ignores its own first 8 segments for 6 ticks.
- **Unused** at expiry: it fizzles with the existing expiry ring. There is no auto-fire, because a random reversal would look like a bug.

**AI.**

- **Escape.** When TRAPPED, run one extra reachable-area query from the tail tip, with the heading pointing out of the tail. Flip if that area is at least 2x the current one. This is rare, so the extra query is affordable.
- **Ambush.** Flip when a rival head is within 6 r of my tail and closing (heading within 45 degrees of it), and I am at least 4 segments longer. Best against Venom chasers.
- **Loot U-turn.** Flip when a death field or prism worth 6 or more is within 15 r behind me, the forward options are worse, and the tail-side area is safe.
- **Rivals** treat a holder's tail tip as a potential head with a 6 r danger cone. Nobody tailgates the false eyes, which reads well and is fair.

**Cost.**

| Element | Vertices | Fragment | CPU / sim |
|---|---|---|---|
| False eyes on the tail | 0 | body branch on effect 6 for `along < 2` (tail pixels only), about 15 ALU | 0 |
| Flip moment | 2 ring quads (existing kinds 12/13, accent) plus one fast wave (existing, white origin) | existing | reversal: about 2 µs at 400 segments, once per use |
| Dizzy eyes | 0 | head mood | 30-tick counter |

**Notes.**

- The renderer clears per-snake history (contrail ring, wave positions) on the Flip event, using the existing "geometry generation changed" path.
- Tail-anchored markings re-anchor at the flip. Chevrons must point at the new head anyway, and the change is hidden under the flip wave.

**Reduced motion.** No glow pulse on the false eyes; rings and wave play 40% shorter.

**Risk.** Medium. Reversal touches the trail ring, the golden traces and every AI forecast that assumes index 0 stays the head. Rivals' forecasts need the tail danger cone above.

### 3.6 Frost, refined (rank 6)

**What it is.** The spec's nova item (section 5.2), with a frozen state worth watching and a combo the AI actually plays.

**Why it's fun.** Rivals turn to ice, shiver with chattering eyes, puff frosty breath and crawl at half speed. The holder uses the window for a boosted cut-off that the slow, wide-turning victim cannot dodge. The survivor thaws (ice cracks off) and flees in fright.

**Rules** (spec rules kept; additions marked):

- At pickup, a nova of 16 base radii. Rivals whose heads are inside become FROZEN for 75 ticks: speed x0.5, turn rate x0.6, no boost, and an active burst is cut. The holder's effect ends at once.
- **New: thaw immunity.** A thawed snake cannot be frozen again for 45 ticks (no permafreeze chains).
- **New: thaw crack.** A small ice-coloured impact (7 short sparks) at the head when FROZEN ends.

**AI.**

- **Value.** Base 3, plus 8 for each rival head forecast to be within 16 r of the item when I arrive.
- **Frost window** (75 ticks). Prefer frozen prey. The existing staged cut-off runs with the prey's turn rate x0.6 and speed x0.5, so cut-offs succeed far more often. This is the combo.
- **Avoid Frost.** If a rival is closer to a Frost item than I am and I am within 18 r of it, move out to more than 16 r.

**Cost.**

| Element | Vertices | Fragment | CPU / sim |
|---|---|---|---|
| Nova | 1 effect quad (new kind 18) | ring, spikes, faint fill | one radius pass over heads at pickup |
| Ice tint | 0 | 0 (vertex colour, 60% toward `#c8eeff`) | CPU colour mix |
| Crystals and sparkles | 0 | body branch on FROZEN (body `packed.y` bit 7): diamonds every 2 segments plus a hashed twinkle; about 15 ALU on frozen bodies only | 0 |
| Shiver, squint, breath | 0 | head mood | 0 |
| Thaw crack | 1 effect quad (kind 6 variant) | existing | 0 |

**Reduced motion.** No shiver, no breath puff, no twinkle (crystals stay).

**Risk.** Low to medium. The turn-rate change feeds AI forecasts, which the spec already covers.

### 3.7 World events: Starfall and Nightfall (rank 7)

**What they are.** Two rare set pieces that change the mood of the whole screen without flashing it.

**Why they're fun.** Starfall is a feeding frenzy with a visible countdown. Nightfall is the calm counterpoint: bodies sink into darkness, the eyes shine like an animal's in headlights, and snakes yawn and doze. Both break minutes of steady state with something to look forward to.

**Starfall rules.**

- **Schedule.** Every 5400-9000 ticks (3-5 min), only when 2 or more snakes are alive. The zone has a radius of 12 base radii and uses the spawn-clearance rule.
- **Telegraph.** 45 ticks: a gold dashed ring with hashed sparkles.
- **Meteors.** 24 meteors arrive over 60 ticks, one every 2.5 ticks. Each flies for 14 ticks from 34 base radii away, along one diagonal per event (world RNG picks one of four). Each lands uniformly in the disk as Star food (value 1.0, life 900 ticks).
- **In flight,** a meteor is a food of kind Meteor: inedible, not an obstacle, moving in a straight line.
- **AI.** The zone is a big target worth 7, with the race rules from 3.2.

**Nightfall rules.**

- **Schedule.** Every 12600-18000 ticks (7-10 min), lasting 750 ticks with 90-tick fades in and out.
- **Look.** An `ambient` uniform scales tube and sheen down to 0.28. Glow gets +0.05, so silhouettes stay. Eyes are not dimmed and gain eyeshine (iris colour, glow 0.5 x night). Food halos x1.6.
- **Sim.** Speed x(1 - 0.1 x night).
- **Moods.** Snakes without a target turn Sleepy and yawn. Hunters keep hunting, so night ambushes happen.
- **Clock.** The clock becomes more readable at night, not less.

**Setting.** A new "World events" checkbox, on by default.

**Cost.**

| Element | Vertices | Fragment | CPU / sim |
|---|---|---|---|
| Starfall zone | 6 (new kind 20) | dashed ring plus 12 hashed sparkles | event scheduler (world RNG at schedule time) |
| Meteors | 12 each (streak plus head), at most 24 for about 2 s, so at most 288 | streak gradient | 24 food moving linearly for 14 ticks |
| Star food | 6 (new kind 21, 4-point twinkle; birth byte gives a landing puff with no extra quad) | about 20 ALU, like a spark | 0 |
| Nightfall | 0 | 1 multiply on tube colour; eyeshine branch about 6 ALU on head pixels | 1 float in the uniform block |

**Reduced motion.** Meteors fade in at their landing spots (no streaks). Zone dashes and sparkles hold still. Nightfall is unchanged (fades are slow).

**Risk.** Low. Watch that Nightfall never looks like a broken display: it is slow, rare, and always keeps the eyes and glows lit.

### 3.8 Whirlpool, new item: brew a feast (rank 8)

**What it is.** Accent `#33e0c8`, icon a spiral. Pickup opens a vortex where the capsule was. Food spirals into it for 5 s and cannot be eaten meanwhile, then it bursts into a ring of essence.

**Why it's fun.** A slow build-up with a known payoff time. The holder coils around its brew like a dragon around gold, rivals circle further out, and the burst sends everyone sweeping.

**Rules.**

- Spawn weight 8, at most one vortex alive (weight 0 while one exists).
- **Vortex.** Radius 22 base radii, lasting 150 ticks.
- **Capture.** Food inside (not prism, seed or meteor) is captured. Each tick it rotates by ω = 0.5 + 1.9q rad/s and is pulled in by R/4 x (0.55 + 0.6q) per second, where q = 1 - r/R. Captured food is not edible and not vacuumable. Pulling stops at tick 138.
- **Absorption.** Inside 0.9 base radii, food is absorbed and its value V accumulated.
- **Burst** at tick 150: n = clamp(round(V/0.7) + 10, 12, 46) shards of value (V + 6)/n each, in a ring with outward speed 70-130 (base-radius scaled), life 14-20 s. The +6 is the item's bonus.

**AI.**

- **Value.** 4 + 0.12 per food within 22 r.
- **Holder.** Orbits at 9 r, so the burst ring crosses its path in the first second.
- **Rivals.** Read the vortex as a big target worth the predicted V, and orbit at 12-15 r.
- **Hunters.** May use the orbiters as cut-off targets.

**Cost.**

| Element | Vertices | Fragment | CPU / sim |
|---|---|---|---|
| Vortex | 6 (new kind 19) | 3 log-spiral arms (one `atan`, one `log`), dashed rim, core glow growing with V, charge ramp over the last 20%; about 30 ALU, additive, arms at alpha 0.32 or less | about 12 flops per captured food per tick (at most about 190 food) |
| Burst | 2 effect quads (ring, small flash, alpha 0.6 or less) | existing | the shards are ordinary food |

**Fill.** The vortex quad is the largest new primitive. At base radius 18 on 4K it is about 790 px across (0.63 MPix, additive). That is why it ranks last and carries its own GPU gate (section 6).

**Reduced motion.** Arms do not spin; food moves straight inward with no tangential motion.

**Risk.** Medium: fill cost on large monitors, and food-ownership edge cases (vacuum claims must release on capture).

---

## 4. Readability, safety and Calm

- **Clock.** Bubbles are never drawn inside the clock's rectangle (renderer only; the sim is unaffected). The shipped scrim covers everything else. Nightfall improves readability.
- **Photosensitivity.** Nothing flashes above 3 Hz over more than a small area. The largest new flash is the Whirlpool burst glow: 5 base radii, alpha 0.6 or less, 0.45 s. The contest pulse is 1.1 Hz, and the frozen shiver is a 0.05-head-unit offset, not a brightness change.
- **Density caps.** Bubbles 3, contest arcs 6, orphans 2, meteors 24, vortex 1, effect quads 8 (unchanged).
- **Calm (reduced motion), in one rule.** Shader time freezes (no spin, shimmer, shiver, wriggle, streak or jitter), discrete events play 40% shorter, and information stays visible: moods, the ripening ring, the drop-in target, contest arcs and the night dimming.
- **Mono and Pastel.** New accents follow the spec's rule (88% toward grey on Mono, 30% toward white on Pastel). Mood colours (angry orange, scared white, frozen ice) fall back to white-greys on Mono, so shape carries the meaning.

---

## 5. Fitting the single-material renderer

One `SnakeMaterial`, one draw call, 6 vertices per primitive. The new ideas use these spare resources:

| Resource | Today | After all slices |
|---|---|---|
| Sprite kinds (`packed.x`) | 0-15 used, 16-31 reserved | +16 bubble, 17 contest arc, 18 nova, 19 vortex, 20 Starfall zone, 21 star food, 22 meteor streak; 23-31 still free |
| Body effect field (3 bits) | 1-5 Surge, Magnet, Phase, Venom, Frost | +6 Flip, 7 Whirlpool: **now full**. A ninth item needs a repack (for example, moving the effect into the free high bits of `packed.w` on body vertices) |
| Body `packed.y` bit 7 | free | FROZEN |
| Head flags bits 1-4 (values 2, 4, 8, 16) | bits 2 and 3 are hunting/trapped; bits 1 and 4 are wave-origin bits, unused on heads | a 4-bit mood enum (hunting and trapped become mood values); body vertices keep their hunting/trapped glow bits |
| Icon atlas (128x128 R8, 16 tiles) | 5 used | +2 icons (Flip, Whirlpool) and +5 glyphs (!, ?, anger, Zz, heart): 12 of 16 |
| Uniform block | matrix, opacity, time, light, animationTime, motionScale, paletteMode | + `ambient` (one float) |
| Per-vertex colour | taper alpha | also ice tint and rainbow hue (CPU) |
| Item spawn weights | Magnet 30, Surge 25, Venom 20, Phase 15, Frost 10 | Magnet 24, Surge 20, Venom 16, Phase 12, Frost 10, Flip 10, Whirlpool 8 (sum 100; the same kind never twice in a row) |

**Sim ABI.** Do all record changes in the first slice, so later slices add behaviour without bumping the ABI again:

- **Snake record:** `mood` (u8), `target_item` (u8), and a STRIKE bit in `flags`.
- **Food kinds:** PrismSeed, Meteor, Star.
- **Event kinds:** Emote, Sever (with the cut index), Flip, Feast, VortexBurst, WorldEvent (start and end for Starfall and Nightfall).
- **World:** `ambient` (f32). The vortex is exported as an item record with a new kind.

If the snake record has no spare bytes, this is ABI v3, and the static asserts and fingerprints are updated once.

**Peak budget** (mature world with everything alive at once, as in the prototype's Chaos scene):

| | Spec's mature estimate | Added at the worst moment |
|---|---|---|
| Vertices | about 8.0k (peak 8.6k) | bubbles 18 + arcs 36 + vortex 6 + zone 6 + meteors 288 + orphan up to 1.2k for 1.1 s, so typically +0.4k and rarely +1.5k |
| Draw calls | 1 (+1 clock scrim) | 0 |
| Full-screen passes | 0 | 0 |
| Sim tick | as shipped | under 0.02 ms per slice, excluding the engineer's race AI |

---

## 6. Build plan: release slices with performance gates

Each slice ships and is measured on its own. The spec's section 9 gates apply to every slice. On top of those:

- **CPU.** Process CPU at or below 1.03x the previous release on the three-monitor seamless setup (100/175/240 Hz), measured with `scripts/frame-timing.sh` and `scripts/benchmark-snakes.sh`. It stays inside today's band.
- **Frame pacing.** Within 1 ms of target at every refresh rate.
- **Renderer.** `benchmarkMatureGeometry` at or below 1.05x the previous release. Add a **chaos fixture** (3 bubbles, 6 arcs, 2 frozen snakes, 2 bulges, 1 orphan, 24 meteors, 1 vortex) and gate its sync time at 1.15x the mature fixture, with at most 9.6k vertices.
- **Simulation.** Mean tick +0.02 ms or less per slice. The zero steady-state allocation test is extended to emotes, orphans, meteors, the vortex and the Flip reversal. Golden and parity traces are regenerated once per slice.
- **GPU.** For slices that add big quads (S6, S7): GPU frame time +0.3 ms or less at base radius 18 on the 3440x1440 monitor.

| Slice | Version | Contents | Extra gate |
|---|---|---|---|
| S1: Faces and races | 0.11.0 | Moods (head shader), emote bubbles and glyph atlas tiles, drop-in telegraph, contest arcs, denied/grudge rule, plus **all ABI record changes** for later slices. Ships with the engineer's race AI. | Head-shader ALU check; bubble cap test; a clock-overlap test (no bubble inside the clock rectangle) |
| S2: Prism and gulp | 0.12.0 | Seed, ripening, Feast event, bulge, rainbow wave, vulture-circling AI, head-on gulp | Vertex count unchanged (bulge and rainbow are 0-vertex) |
| S3: Venom | 0.13.0 | Severing, bite immunity, strike pose, stump, orphan wriggle, standoff counter-play | Orphan buffer allocation test; titan-tail cut fixture |
| S4: Frost | 0.14.0 | Nova, FROZEN bit and looks, thaw immunity and crack, frost-window and avoid-Frost AI | Duel suite: frozen-prey cut-off success rate reported |
| S5: Flip | 0.15.0 | Held charge, in-place reversal plus `rebuild_trail`, false eyes, escape/ambush/loot AI, tail danger cone | Reversal fuzz test (wrap and walls); forecast consistency after a flip |
| S6: Starfall and Nightfall | 0.16.0 | Event scheduler, meteors, star food, `ambient` uniform, eyeshine, sleepy moods, "World events" setting | GPU gate; a 30-minute soak shows both events firing on schedule |
| S7: Whirlpool | 0.17.0 | Vortex record, capture and burst, orbit AI, spawn exclusivity | GPU fill gate at 4K; food-ownership tests |

**Backlog**, in order: Rampage streaks, Hatching and molting, Seasonal touches (a candidate quick win in October: a jack-o'-lantern prism and orange eyeshine at Nightfall, behind a "Seasonal touches" setting).

**Rollback.** As in the spec: if a slice misses a gate, its visuals ship behind a disabled default, or the slice waits. A regression is never shipped.

---

## 7. Decisions for the user

1. **Order.** Faces and races go before Venom and Frost, unlike the spec's R4. Reason: they are the cheapest slice with the biggest effect, and they make the engineer's racing AI visible as soon as it lands. Prism comes next as the race prize.
2. **Two new items** (Flip, Whirlpool) fill the 3-bit effect field. An eighth item means a small repack.
3. **Bulge** is the one deliberate exception to "what you see is what kills you" (visual only, at most +35%, transient). The alternative is to make collisions follow it at a small per-segment cost. The recommendation is the visual exception.
4. **World events** get their own setting, on by default.
5. **Emote frequency** is capped (3 on screen, 5 s per snake, one reaction per event). If it still feels busy in the real thing, lower the cap to 2 before cutting glyphs.

# R3 effect hooks (step A)

The item system owns spawning, pickup, replacement and timed expiry. The three
modules `surge.rs`, `magnet.rs` and `phase.rs` intentionally implement inert
`EffectHook`s. `ENABLED_KINDS` controls spawning, with the design weights
renormalized over enabled kinds excluding the previous spawn. Add Venom/Frost
there in R4 after supplying their hooks.

Workers implement their module's existing unit struct and `EffectHook`:

- `activate(&mut World, snake_id)` runs after assigning effect kind/full duration.
- `tick(&mut World, snake_id)` runs before steering/movement, after decrement,
  only while remaining ticks are positive.
- `end(&mut World, snake_id, EndReason)` runs on replacement, expiry, disabling
  or death. The old kind is still present; expiry has zero remaining ticks.
- `modifiers(remaining_ticks)` supplies pure speed multiplier, food pull reach in
  body radii, free boost, boost cooldown, body/head intangibility and ABI flags.
  Neutral defaults are speed 1, reach 3r, paid boost, cooldown 36, tangible,
  flags 0. World movement, feeding, boost, collisions and motion predictions
  already consume these controls. Walls always remain lethal.
- `ai_bonus(&World, SnakeView, item_position)` adds to that kind's base item value.
- `ai_steering(&World, SnakeView, Steering)` can adjust the final smart-AI
  control for active-effect policy. It never runs for scripted steering.

All hooks must remain allocation-free and deterministic. Do not draw RNG in a
hook. Effects have 180/300/120 ticks for Surge/Magnet/Phase. `WARNING_TICKS` is
36. Item `life_ticks` means remaining lifetime; birth lifetime is 750 ticks.

AI items occupy three fixed target slots after food, with the high bit set on
item IDs. They use food's arrival, competition, sticky target, routing and boost
race handling. Kind-specific threat prediction and Phase expiry escape planning
are for the effect workers; the step-A mechanics modifiers alone do not implement
those policies.

ABI v2's previously empty item/effect slots are now populated. All existing
entry points, record sizes and member offsets are preserved. The config's old
reserved word has a C `power_ups` alias: 0 default/on, bit 31 off; other bits must
be zero. Rust's `CoreConfig::from(Config)` writes the word automatically. Config
remains 80 bytes and items remain 24 bytes. `snakes_core_item_radius(world)`
exports the shared physical capsule radius. `snakes_core_render_set_items`
copies up to three records plus that radius into fixed per-renderer storage
before an existing build call; no borrowed storage is retained. Item event kinds are 5 spawn, 6 field expiry, 7 effect expiry; pickup
remains 2. For these events `other_snake_id` carries the effect kind rather than
a rival ID, and field-item `snake_id` is UINT32_MAX.

Shader sprite kinds are 11 item, 12 pickup ring, 13 collapsing ring and 14 head
expiry warning. Every item has six vertices; rings/warnings share the eight
visible effect-quad cap. Body payload bits 2..4 preserve the active effect kind; body-only flag bit 1
marks a live pickup wave (cooldown has no body visual). The build-time atlas has five 32px tiles in a 128x128 R8 texture, loaded
once per material. Classic fallback tessellates capsules/icons/rings in the
existing vertex-colour material, and tints existing body edges for pickup waves.

Steering sees effect counters after the current tick's decrement. Shared
`remaining_ticks(ticks, step)` treats step one as the current movement and
`ticks + 1` as the first expired movement. AI motion lives in `motion.rs`:
burst price is latched before projection, effect expiry does not charge an
ongoing free burst, and boost/cooldown/Frost ordering follows `advance_boost`.
All effect-aware AI queries use this convention, including future food bonuses
and both parties' Phase contact lethality.

S3 adds Venom to `ENABLED_KINDS` with duration 240. Its neutral movement/feed
modifiers deliberately use the default hook: bite dispatch belongs to the body
collision pass (`world/venom.rs`), after endpoint pickups, never to unchecked
final AI steering. Immunity/stump state lives in the cold face array, keeping
copied motion Snake records unchanged. Sever food is delayed in reserved storage.

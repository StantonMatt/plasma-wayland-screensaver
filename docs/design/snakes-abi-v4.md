# Snakes ABI v4: power-up inventory (0.18.0)

v4 preserves every v3 function signature and all preceding record offsets.
Consumers must check `snakes_core_abi_version() == 4` and rebuild with the v4
header; the snake array stride changes from 152 to 160 bytes. Other records
retain their v3 sizes (config 80, steering 32, item 88, event 56, frame info 128).
The historical v3 contract is in [snakes-abi-v3.md](snakes-abi-v3.md).

| Snake byte offset | Field | Contract |
| --- | --- | --- |
| 152 | `uint8_t inv_kind[3]` | Ordered held kinds, 1 Surge, 2 Magnet, 3 Phase, 4 Venom, 5 Frost; unused slots zero |
| 155 | `uint8_t inv_count` | 0..3, duplicates allowed |
| 156 | `uint8_t inv_life[3]` | 255 until the final 150 ticks; then ceil(remaining * 255 / 150); unused slots zero |
| 159 | `uint8_t inv_windup` | 0 idle, selected slot+1 (1..3) during the four-tick wind-up |

The selected slot remains in `inv_kind` and counts toward capacity until
activation. Render its in-flight pip using the Use event rather than drawing it
twice. At activation that slot is removed and later slots compact in order.
Use events carry generation, the zero-based chosen slot in `cut_index`, kind
in `other_snake_id`, head position and `duration_ticks=4`. Activation occurs at
the endpoint four ticks after the request (after movement/feed, before capsule
contact and collisions); the old effect remains live during the wind-up.
The existing Pickup=2 event marks activation and drives the wave/use ring.
Event byte 29 (previously reserved) is now `uint8_t flags`; bit 0 is
`SNAKES_CORE_EVENT_HELD_ACTIVATION`. Only a held-completion Pickup sets this
bit and carries the removed zero-based slot in `cut_index`. Field-contact
Pickup and Stash leave it clear; all other bits and the remaining two reserved
bytes stay zero. The event remains 56 bytes with all payload offsets unchanged.
Consumers must use this source flag to distinguish held completion from field
acquisition, even when owner, generation, tick, kind and position coincide.
Render compaction follows flagged held completion in event order; a later touch
Use/Pickup cannot suppress it. Diagnostics also confirm that a matched capsule
ID is absent from the post-step field before counting it as consumed.
Frost also emits the existing Nova=3. Instant full-inventory contacts emit
Use (duration 0, cut_index=65535) and Pickup immediately, with no wind-up. At most one held request is accepted
per 30 ticks; full-inventory touch remains the instant-use exception.

| Event kind | Name | Payload |
| --- | --- | --- |
| 13 | Use | Accepted held request; duration 4, zero-based slot, effect kind, owner generation and head position |
| 14 | Stash | Capsule stored; zero-based destination slot, effect kind, owner generation and pickup position |
| 15 | Fizzle | Expired/disabled/cap-rejected held item; zero-based slot, effect kind, owner generation, pip position, duration 8 |

`steering.actions` at offset 24 is an integer selection, not a bit mask:
0 does nothing; 1, 2, 3 request slots 0, 1, 2. Values >3 and nonzero reserved
are invalid. Empty slots, disabled storage, cooldown and a pending request
ignore the request without changing steering. Scripted generation matching
also applies to actions. Rust controllers expose the same selection through
`Controller::use_request`, keeping the existing Rust `Steering` record intact.

Config word at byte 76 gains `SNAKES_CORE_INVENTORY_OFF = 0x10000000` (bit 28).
Zero means on, including when reading old configuration files. Qt uses the
property `snakeStorePowerUps`, persisted as `SnakeStorePowerUps`. Power-ups off
and Classic disable inventory mechanics. Turning storage off fizzles all held
slots and restores immediate contact activation. Frost is instant and never
replaces the holder's active Surge/Magnet/Phase/Venom, in either toggle mode.

Held items expire after 1800 ticks, independent of effects. On death all held
slots, including a winding-up slot, attempt deterministic drops at arc offsets
2.2r/4.3r/6.4r, pushed 3r sideways (alternating) or backward for slot 3.
Drops emit ItemSpawn=5 with the dying owner ID/generation, zero-based slot
in cut_index and duration 15; the new ItemRecord has `state=1`,
300 remaining life ticks and `pickable_from_tick=death_tick+15`. The existing
world capsule cap applies; excess slots emit Fizzle instead. No RNG is drawn.
Respawns start empty. IDs remain monotonic and generation-safe.

Item bubble glyphs 5..9 are Surge/Magnet/Phase/Venom/Frost (kind+4), with
45-tick lifetime. They replace their owner's bubble and obey the existing
three-bubble cap, using the same forced replacement policy as Anger/Heart.
Rendering (pips, icon mapping, drain, flight and Surge styling) is a separate
worker's scope; this document specifies the simulation data it consumes.

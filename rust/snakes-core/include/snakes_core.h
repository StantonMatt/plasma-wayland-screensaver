/* SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef SNAKES_CORE_H
#define SNAKES_CORE_H
#include <stddef.h>
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
#define SNAKES_CORE_ABI_VERSION 3u
#define SNAKES_CORE_RULE_DEFAULT 0u /* V2 */
#define SNAKES_CORE_RULE_CLASSIC 1u
#define SNAKES_CORE_RULE_V2 2u
#define SNAKES_CORE_BOOSTING 1u
#define SNAKES_CORE_COOLDOWN 2u
#define SNAKES_CORE_HUNTING 4u
#define SNAKES_CORE_TRAPPED 8u
#define SNAKES_CORE_FROZEN 16u
#define SNAKES_CORE_PHASED 32u
#define SNAKES_CORE_LEADER 64u
#define SNAKES_CORE_CORPSE 128u
#define SNAKES_CORE_FOOD_SPARK 0u
#define SNAKES_CORE_FOOD_SHARD 1u
#define SNAKES_CORE_FOOD_PELLET 2u
#define SNAKES_CORE_FOOD_PRISM 3u
#define SNAKES_CORE_EVENT_KILL 0u
#define SNAKES_CORE_EVENT_SEVER 1u
#define SNAKES_CORE_EVENT_PICKUP 2u
#define SNAKES_CORE_EVENT_NOVA 3u
#define SNAKES_CORE_EVENT_SUCCESSION 4u
#define SNAKES_CORE_MAX_EVENTS 32u
#define SNAKES_CORE_MAX_SNAKES 14u
#define SNAKES_CORE_MAX_FOOD 480u
#define SNAKES_CORE_MAX_ITEMS 4u /* 3 capsules + 1 future vortex */
#define SNAKES_CORE_MAX_CAPSULES 3u
#define SNAKES_CORE_POWER_UPS_DEFAULT 0u
#define SNAKES_CORE_POWER_UPS_ON 0u
#define SNAKES_CORE_POWER_UPS_OFF 0x80000000u
#define SNAKES_CORE_EFFECT_NONE 0u
#define SNAKES_CORE_EFFECT_SURGE 1u
#define SNAKES_CORE_EFFECT_MAGNET 2u
#define SNAKES_CORE_EFFECT_PHASE 3u
#define SNAKES_CORE_EFFECT_VENOM 4u
#define SNAKES_CORE_EFFECT_FROST 5u
#define SNAKES_CORE_ACTION_FLIP 1u
#define SNAKES_CORE_EFFECT_FLIP 6u
#define SNAKES_CORE_EFFECT_WHIRLPOOL 7u
#define SNAKES_CORE_ITEM_VORTEX 8u
#define SNAKES_CORE_WORLD_EVENTS_OFF 0x40000000u
#define SNAKES_CORE_ITEM_LANDING_TICKS 30u
#define SNAKES_CORE_PRISM_RIPEN_TICKS 90u
#define SNAKES_CORE_BUBBLE_LIFE_TICKS 45u
#define SNAKES_CORE_MAX_BUBBLES 3u
#define SNAKES_CORE_MAX_CONTENDERS 2u
#define SNAKES_CORE_NO_SNAKE UINT32_MAX
#define SNAKES_CORE_NO_ITEM 255u
#define SNAKES_CORE_FLAG_STRIKE 256u
#define SNAKES_CORE_FLAG_FLIP_HELD 512u
#define SNAKES_CORE_FACE_PRISM_TARGET 4u /* committed to the one live seed/fruit */
#define SNAKES_CORE_FACE_OBSERVED 2u /* authoritative mood, including Calm=0 */
#define SNAKES_CORE_MOOD_CALM 0u
#define SNAKES_CORE_MOOD_SLEEPY 1u
#define SNAKES_CORE_MOOD_HUNTING 2u
#define SNAKES_CORE_MOOD_SCARED 3u
#define SNAKES_CORE_MOOD_ANGRY 4u
#define SNAKES_CORE_MOOD_HAPPY 5u
#define SNAKES_CORE_MOOD_TRAPPED 6u
#define SNAKES_CORE_MOOD_DIZZY 7u
#define SNAKES_CORE_MOOD_FROZEN 8u
#define SNAKES_CORE_GLYPH_ALERT 0u
#define SNAKES_CORE_GLYPH_QUESTION 1u
#define SNAKES_CORE_GLYPH_ANGER 2u
#define SNAKES_CORE_GLYPH_SLEEP 3u
#define SNAKES_CORE_GLYPH_HEART 4u
#define SNAKES_CORE_FOOD_PRISM_SEED 4u
#define SNAKES_CORE_FOOD_METEOR 5u
#define SNAKES_CORE_FOOD_STAR 6u
#define SNAKES_CORE_EVENT_EMOTE 8u
#define SNAKES_CORE_EVENT_FLIP 9u
#define SNAKES_CORE_EVENT_FEAST 10u
#define SNAKES_CORE_EVENT_VORTEX_BURST 11u
#define SNAKES_CORE_EVENT_WORLD_EVENT 12u
#define SNAKES_CORE_WORLD_EVENT_NONE 0u
#define SNAKES_CORE_WORLD_EVENT_STARFALL 1u
#define SNAKES_CORE_WORLD_EVENT_NIGHTFALL 2u
#define SNAKES_CORE_EVENT_ITEM_SPAWN 5u
#define SNAKES_CORE_EVENT_ITEM_EXPIRY 6u
#define SNAKES_CORE_EVENT_EFFECT_EXPIRY 7u
/* Config bit 31 disables power-ups; bit 30 disables world events. Both
 * default on (word zero). All other reserved bits/fields must be zero. */
#define SNAKES_CORE_OK 0
#define SNAKES_CORE_INVALID_ARGUMENT 1
#define SNAKES_CORE_BUFFER_TOO_SMALL 2
/* All calls on a handle must be serialized. Handles must be live. All pointer
 * storage must be valid, aligned, and disjoint. Null arrays are accepted only
 * when their required count is zero. No function retains caller buffers.
 * width/height: [80,16384]; numeric controls: [0,1000]; palette: [1,4096].
 * intelligence is clamped to 100 internally; booleans are exactly 0 or 1.
 * Fixed step: 1/30 second. These are mechanics, without the QML AI. */
typedef struct snakes_core_world snakes_core_world;
typedef struct snakes_core_config {
    double width, height, density, trails, scale, speed, intelligence;
    int32_t seed;
    uint32_t palette_size, self_collisions, deadly_walls;
    uint32_t rule_set;
    union { uint32_t reserved; uint32_t power_ups; }; /* 0 default/on, bit 31 off */
} snakes_core_config;
typedef struct snakes_core_steering_input {
    uint32_t id, generation;
    double desired_angle, rush; /* Classic: rush in [0,1]; V2: rush>0 requests boost; generation 0 matches any */
    uint32_t actions, reserved; /* action bit 0 requests held Flip; inert in step A; reserved zero */
} snakes_core_steering_input;
/* Tick-based visual-only bulge: centre travels from origin_segment toward the
 * tail over duration_ticks; strength is fractional widening (0..0.35). Zero
 * duration disables a slot. No collision geometry changes. */
typedef struct snakes_core_bulge {
    uint64_t start_tick;
    uint16_t duration_ticks, origin_segment;
    float strength;
} snakes_core_bulge;
/* Persistent, capped bubble snapshot. Age 0..44; identity includes generation.
 * Glyph also travels in Emote.other_snake_id. reserved must remain zero. */
typedef struct snakes_core_bubble {
    uint32_t snake_id, generation;
    uint16_t age_ticks;
    uint8_t glyph, reserved;
} snakes_core_bubble;
/* Reserved world event state. Tick window and zone are world coordinates.
 * night: 0..1 fade amount; ambient: 0..1 tube brightness (1 by default).
 * phase: 0 inactive, 1 telegraph, 2 active, 3 fading; meteor_count <=24.
 * Starfall and Nightfall may overlap: night/ambient persist independently of
 * the Starfall zone window. All schedulers are inert in step A. */
typedef struct snakes_core_world_event {
    uint64_t start_tick, end_tick;
    float x, y, radius, night, ambient;
    uint8_t kind, phase, meteor_count, reserved;
} snakes_core_world_event;
typedef struct snakes_core_snake {
    uint32_t id, generation, alive, color_index;
    double radius, angle, desired_angle;
    uint32_t segment_offset, segment_count;
    uint32_t flags;
    uint16_t effect_ticks;
    uint8_t effect_kind, boost_ticks; /* remaining ticks, including latest tick */
    uint8_t mood, mood_intensity; /* enum above; intensity 0..255 onset ramp */
    uint16_t mood_age_ticks; /* saturating age of current mood, zero on switch */
    uint8_t target_item, face_flags; /* compact current slot/255; bit 0 guarding, bit 1 authoritative mood (including Calm), bit 2 prism target */
    uint16_t jaw_ticks; /* remaining yawn/strike jaw animation ticks */
    float look_x, look_y; /* wrapped world-space DELTA head -> look target */
    float pupil_x, pupil_y; /* head-local offset in head radii */
    uint16_t frozen_ticks, dizzy_ticks, bite_immunity_ticks, stump_ticks;
    uint16_t thaw_immunity_ticks, breath_ticks, flip_grace_ticks, happy_ticks;
    /* Above counters reserved for Frost, Flip, Venom, breath and happy blep.
     * happy_ticks: >18 means the first 27 ticks of the 45-tick celebration. */
    uint32_t grudge_snake_id; /* UINT32_MAX if absent; valid only with ticks */
    uint16_t grudge_ticks, reserved; /* remaining; reserved zero */
    uint32_t grudge_generation; /* prevents grudges following a respawn */
    uint64_t flip_tick; /* last in-place reversal; 0 before any Flip */
    snakes_core_bulge bulges[2]; /* fixed slots, inactive in step A */
} snakes_core_snake;
typedef struct snakes_core_segment { float x, y, previous_x, previous_y; } snakes_core_segment;
typedef struct snakes_core_food {
    uint64_t id;
    float x, y, size, phase, attraction, attraction_x, attraction_y;
    uint32_t color_index;
    uint8_t kind, life_fraction; /* 0 expired, 255 full; vacuum locks lifetime */
    uint16_t reserved;
    uint64_t ripe_tick; /* PrismSeed's absolute edible tick; 0 for ordinary food */
    float motion_origin_x, motion_origin_y; /* Meteor launch point, world units */
    uint16_t motion_ticks, captured_by; /* flight countdown; 0 free, vortex slot+1 */
    uint32_t food_flags; /* prism race: IDs+1 bits 0..3/4..7, second leads bit 8, contested bit 9; otherwise zero */
} snakes_core_food;
typedef struct snakes_core_item {
    uint64_t id;
    float x, y;
    uint8_t kind, reserved_byte;
    uint16_t age_ticks, life_ticks, reserved; /* life_ticks: remaining incl landing */
    uint64_t pickable_from_tick; /* first completed endpoint eligible for pickup */
    uint32_t leader_snake_id; /* best ETA committed racer, UINT32_MAX if none */
    float leader_eta; /* seconds, distance/speed + abs(heading error)/turn rate */
    uint16_t landing_ticks; /* max(pickable_from_tick - frame.tick, 0), <=30 */
    uint8_t contender_count, state; /* <=2; state reserved for vortex phase */
    uint32_t contender_ids[2]; /* two nearest committed heads; unused UINT32_MAX */
    float contender_etas[2]; /* seconds; unused +infinity; leader may be outside */
    uint32_t guard_snake_id; /* held-effect guard, UINT32_MAX if none */
    float radius; /* physical capsule / vortex radius, world units */
    float captured_value; /* reserved Whirlpool absorbed nutrition */
    uint16_t charge_ticks, reserved_v3; /* vortex remaining charge; reserved zero */
    uint32_t owner_generation; /* reserved vortex owner lifetime identity */
    uint32_t owner_snake_id, reserved_owner; /* UINT32_MAX when absent; reserved zero */
} snakes_core_item;
typedef struct snakes_core_event {
    uint64_t tick;
    float x, y;
    uint32_t snake_id, other_snake_id, color_index;
    uint8_t kind, reserved[3];
    uint16_t cut_index, duration_ticks; /* Sever's first removed segment; animation life */
    uint32_t generation, other_generation; /* actor/other identity, zero when unused */
    float value; /* Feast/VortexBurst nutrition, zero otherwise */
    uint64_t release_tick; /* delayed Sever essence release / world event endpoint */
} snakes_core_event;
/* Kill: snake_id victim, other_snake_id first rival owner (UINT32_MAX for
 * wall/self). Succession: new/previous leader (UINT32_MAX for no predecessor).
 * tick is the completed physics tick; initial succession may have tick 0.
 * Corpses contribute to frame segments, never to statistics.total_segments. */
typedef struct snakes_core_frame_sizes { uint32_t snakes, segments, food, items, events, reserved; } snakes_core_frame_sizes;
typedef struct snakes_core_frame_info {
    uint64_t tick;
    double simulation_time, world_width, world_height;
    uint64_t geometry_generation;
    float ambient; /* 1 normal; future Nightfall fades to 0.28 */
    uint32_t bubble_count; /* <=3; persistent snapshot, independent of event ring */
    snakes_core_bubble bubbles[3]; /* only prefix bubble_count is live */
    snakes_core_world_event world_event;
} snakes_core_frame_info;
typedef struct snakes_core_statistics {
    uint32_t alive, total_segments, food, reserved;
    uint64_t deaths, wall_deaths, head_deaths, body_deaths, self_deaths;
} snakes_core_statistics;
uint32_t snakes_core_abi_version(void);
/* Output is set to NULL on invalid configuration. Destruction accepts NULL. */
int32_t snakes_core_create(const snakes_core_config *config, snakes_core_world **output);
void snakes_core_destroy(snakes_core_world *world);
/* Count, seed or rule-set changes restart the world. Other changes preserve state. */
int32_t snakes_core_reconfigure(snakes_core_world *world, const snakes_core_config *config);
int32_t snakes_core_resize(snakes_core_world *world, double width, double height);
/* Persistent inputs replace the table atomically. IDs must be unique/in range.
 * Omitted snakes continue straight; length 0 restores the temporary baseline.
 * Resend a table each tick for recorded playback. */
int32_t snakes_core_set_steering(snakes_core_world *world, const snakes_core_steering_input *inputs, size_t length);
int32_t snakes_core_step(snakes_core_world *world, uint32_t ticks); /* <= 1000000 */
int32_t snakes_core_get_frame_sizes(const snakes_core_world *world, snakes_core_frame_sizes *output);
/* Capacities are record counts. Insufficient capacity leaves every output
 * untouched. Frame offsets are compact and include dead snake records. */
int32_t snakes_core_export_frame(const snakes_core_world *world,
    snakes_core_snake *snakes, size_t snake_capacity,
    snakes_core_segment *segments, size_t segment_capacity,
    snakes_core_food *food, size_t food_capacity, snakes_core_frame_info *info);
/* Shared physical capsule radius; invalid/null world returns zero. */
double snakes_core_item_radius(const snakes_core_world *world);
/* Latest tick's event ring, oldest first, at most MAX_EVENTS. Reading does not
 * consume it; step(world,n) retains only the last tick. Overflow evicts oldest.
 * Failure changes no output. For pickup/spawn/item expiry/effect expiry,
 * other_snake_id is the effect kind; snake_id is UINT32_MAX for field items.
 * Emote: snake_id owner, other_snake_id glyph. Sever: victim/biter and cut_index.
 * Flip/Feast: actor. WorldEvent: snake_id UINT32_MAX, other_snake_id event kind,
 * duration_ticks window; value 1 start / 0 end. Vortex uses item kind 8 (not
 * effect kind 7); age/life/charge, captured_value and owner identity are reserved. */
int32_t snakes_core_export_extras(const snakes_core_world *world,
    snakes_core_item *items, size_t item_capacity,
    snakes_core_event *events, size_t event_capacity);
int32_t snakes_core_stats(const snakes_core_world *world, snakes_core_statistics *output);
/* Optional AI overlay API. Empty steering tables restore the smart default AI. */
typedef struct snakes_core_ai_debug_point { float x, y; } snakes_core_ai_debug_point;
typedef struct snakes_core_ai_debug_record {
    uint32_t id, generation, target_count, path_count, flags, reachable_cells;
    double safe_seconds;
    uint64_t target_food_ids[5];
    snakes_core_ai_debug_point path[16];
} snakes_core_ai_debug_record;
/* Flags: 1 capped area, 2 capped safety, 4 no safe horizon, 8 interception.
 * Dead/unplanned/scripted slots have zero counts. Invalid IDs return INVALID.
 * target_food_ids with bit 63 set identify items (lower bits are item ID). */
/* Additional flag: 16 means the retained plan was checked against current
 * hazards and reused until the next strategy decision. 32 means trapped. */
int32_t snakes_core_ai_debug(const snakes_core_world *world, uint32_t id,
                           snakes_core_ai_debug_record *output);
/* Per-window classic rendering. Vertices have exactly Qt ColoredPoint2D's
 * x,y float + RGBA bytes layout. One batch of unindexed triangles.
 * create allocates fixed scratch/history; build never allocates. All calls on
 * a render handle must be serialized; destroy accepts NULL. reset clears visual
 * history on simulation replacement; rewind/geometry-generation changes reset
 * automatically. Buffers are caller-owned and never retained.
 * build validates arguments before changing output/history. BUFFER_TOO_SMALL
 * writes a prefix and the required count, and advances history once; grow the
 * buffer and retry the same frame. Counts are records, not bytes.
 * The entire vertex capacity must be initialized (zero-filled is valid),
 * including its unused tail. Initialize on allocation, then reuse. Other
 * output buffers may be uninitialized; their records are written directly. */
typedef struct snakes_core_renderer snakes_core_renderer;
typedef struct snakes_core_render_color { uint8_t red, green, blue, alpha; } snakes_core_render_color;
typedef struct snakes_core_render_vertex { float x, y; snakes_core_render_color color; } snakes_core_render_vertex;
typedef struct snakes_core_render_params {
    double viewport_width, viewport_height, scale_x, scale_y, offset_x, offset_y;
    double interpolation, presentation_time;
    uint32_t deadly_walls, developer_mode;
} snakes_core_render_params;
typedef struct snakes_core_render_output { size_t vertex_count; uint32_t dense_food, reserved; } snakes_core_render_output;
// Single-pass shader geometry. Params: kind, tier/effect, flags, wave/look.
typedef struct snakes_core_shader_vertex {
    float x, y, across, along;
    snakes_core_render_color color;
    uint8_t params[4];
} snakes_core_shader_vertex;
/* Local monitor pixels; empty width/height disables clock exclusion. */
int32_t snakes_core_render_set_clock_rect(snakes_core_renderer *renderer,
    double x, double y, double width, double height);
int32_t snakes_core_render_set_reduced_motion(snakes_core_renderer *renderer, uint32_t enabled);
int32_t snakes_core_render_build_shader(snakes_core_renderer *renderer,
    const snakes_core_frame_info *info,
    const snakes_core_snake *snakes, size_t snake_count,
    const snakes_core_segment *segments, size_t segment_count,
    const snakes_core_food *food, size_t food_count,
    const snakes_core_event *events, size_t event_count,
    const snakes_core_render_color *palette, size_t palette_count,
    const snakes_core_render_params *params, snakes_core_shader_vertex *vertices,
    size_t vertex_capacity, snakes_core_render_output *output);
/* Fixed per-renderer snapshot. Call before build; count <= MAX_ITEMS.
 * Radius is snakes_core_item_radius(world); coordinates are world space. */
int32_t snakes_core_render_set_items(snakes_core_renderer *renderer,
    const snakes_core_item *items, size_t count, double radius);
snakes_core_renderer *snakes_core_render_create(void);
void snakes_core_render_destroy(snakes_core_renderer *renderer);
int32_t snakes_core_render_reset(snakes_core_renderer *renderer);
int32_t snakes_core_render_build(snakes_core_renderer *renderer,
    const snakes_core_frame_info *info,
    const snakes_core_snake *snakes, size_t snake_count,
    const snakes_core_segment *segments, size_t segment_count,
    const snakes_core_food *food, size_t food_count,
    const snakes_core_event *events, size_t event_count,
    const snakes_core_render_color *palette, size_t palette_count,
    const snakes_core_render_params *params, snakes_core_render_vertex *vertices,
    size_t vertex_capacity, snakes_core_render_output *output);
#ifdef __cplusplus
}
#endif
#if defined(__cplusplus)
#define SNAKES_CORE_ASSERT(c) static_assert(c, #c)
#else
#define SNAKES_CORE_ASSERT(c) _Static_assert(c, #c)
#endif
SNAKES_CORE_ASSERT(sizeof(snakes_core_config) == 80);
SNAKES_CORE_ASSERT(sizeof(snakes_core_steering_input) == 32);
SNAKES_CORE_ASSERT(sizeof(snakes_core_snake) == 152);
SNAKES_CORE_ASSERT(sizeof(snakes_core_segment) == 16);
SNAKES_CORE_ASSERT(offsetof(snakes_core_steering_input, actions) == 24);
SNAKES_CORE_ASSERT(sizeof(snakes_core_bulge) == 16);
SNAKES_CORE_ASSERT(sizeof(snakes_core_bubble) == 12);
SNAKES_CORE_ASSERT(sizeof(snakes_core_world_event) == 40);
SNAKES_CORE_ASSERT(offsetof(snakes_core_snake, mood) == 56);
SNAKES_CORE_ASSERT(offsetof(snakes_core_snake, look_x) == 64);
SNAKES_CORE_ASSERT(offsetof(snakes_core_snake, flip_tick) == 112);
SNAKES_CORE_ASSERT(offsetof(snakes_core_snake, bulges) == 120);
SNAKES_CORE_ASSERT(offsetof(snakes_core_food, ripe_tick) == 48);
SNAKES_CORE_ASSERT(offsetof(snakes_core_item, pickable_from_tick) == 24);
SNAKES_CORE_ASSERT(offsetof(snakes_core_item, contender_ids) == 44);
SNAKES_CORE_ASSERT(offsetof(snakes_core_event, cut_index) == 32);
SNAKES_CORE_ASSERT(offsetof(snakes_core_frame_info, bubbles) == 48);
SNAKES_CORE_ASSERT(offsetof(snakes_core_frame_info, world_event) == 88);
SNAKES_CORE_ASSERT(sizeof(snakes_core_food) == 72);
SNAKES_CORE_ASSERT(sizeof(snakes_core_frame_sizes) == 24);
SNAKES_CORE_ASSERT(sizeof(snakes_core_frame_info) == 128);
SNAKES_CORE_ASSERT(sizeof(snakes_core_statistics) == 56);
SNAKES_CORE_ASSERT(offsetof(snakes_core_snake, radius) == 16);
SNAKES_CORE_ASSERT(offsetof(snakes_core_food, color_index) == 36);
SNAKES_CORE_ASSERT(offsetof(snakes_core_statistics, deaths) == 16);
SNAKES_CORE_ASSERT(sizeof(snakes_core_item) == 88);
SNAKES_CORE_ASSERT(sizeof(snakes_core_event) == 56);
SNAKES_CORE_ASSERT(offsetof(snakes_core_config, rule_set) == 72);
SNAKES_CORE_ASSERT(offsetof(snakes_core_config, power_ups) == 76);
SNAKES_CORE_ASSERT(offsetof(snakes_core_snake, flags) == 48);
SNAKES_CORE_ASSERT(offsetof(snakes_core_snake, effect_ticks) == 52);
SNAKES_CORE_ASSERT(offsetof(snakes_core_snake, effect_kind) == 54);
SNAKES_CORE_ASSERT(offsetof(snakes_core_snake, boost_ticks) == 55);
SNAKES_CORE_ASSERT(offsetof(snakes_core_food, kind) == 40);
SNAKES_CORE_ASSERT(offsetof(snakes_core_food, life_fraction) == 41);
SNAKES_CORE_ASSERT(offsetof(snakes_core_item, age_ticks) == 18);
SNAKES_CORE_ASSERT(offsetof(snakes_core_item, life_ticks) == 20);
SNAKES_CORE_ASSERT(offsetof(snakes_core_event, snake_id) == 16);
SNAKES_CORE_ASSERT(offsetof(snakes_core_event, kind) == 28);
SNAKES_CORE_ASSERT(sizeof(snakes_core_ai_debug_point) == 8);
SNAKES_CORE_ASSERT(sizeof(snakes_core_ai_debug_record) == 200);
SNAKES_CORE_ASSERT(offsetof(snakes_core_ai_debug_record, target_food_ids) == 32);
SNAKES_CORE_ASSERT(offsetof(snakes_core_ai_debug_record, path) == 72);
SNAKES_CORE_ASSERT(sizeof(snakes_core_shader_vertex) == 24);
SNAKES_CORE_ASSERT(offsetof(snakes_core_shader_vertex, color) == 16);
SNAKES_CORE_ASSERT(offsetof(snakes_core_shader_vertex, params) == 20);
SNAKES_CORE_ASSERT(sizeof(snakes_core_render_color) == 4);
SNAKES_CORE_ASSERT(sizeof(snakes_core_render_vertex) == 12);
SNAKES_CORE_ASSERT(offsetof(snakes_core_render_vertex, color) == 8);
SNAKES_CORE_ASSERT(sizeof(snakes_core_render_params) == 72);
SNAKES_CORE_ASSERT(sizeof(snakes_core_render_output) == sizeof(size_t) + 8);
#undef SNAKES_CORE_ASSERT
#endif

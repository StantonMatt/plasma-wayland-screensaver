/* SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef SNAKES_CORE_H
#define SNAKES_CORE_H
#include <stddef.h>
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
#define SNAKES_CORE_ABI_VERSION 2u
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
/* Effect/item kinds reserved for R3/R4: none 0, Surge 1, Magnet 2,
 * Phase 3, Venom 4, Frost 5. Reserved fields must be zero. */
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
    uint32_t rule_set, reserved;
} snakes_core_config;
typedef struct snakes_core_steering_input {
    uint32_t id, generation;
    double desired_angle, rush; /* Classic: rush in [0,1]; V2: rush>0 requests boost; generation 0 matches any */
} snakes_core_steering_input;
typedef struct snakes_core_snake {
    uint32_t id, generation, alive, color_index;
    double radius, angle, desired_angle;
    uint32_t segment_offset, segment_count;
    uint32_t flags;
    uint16_t effect_ticks;
    uint8_t effect_kind, boost_ticks; /* remaining ticks, including latest tick */
} snakes_core_snake;
typedef struct snakes_core_segment { float x, y, previous_x, previous_y; } snakes_core_segment;
typedef struct snakes_core_food {
    uint64_t id;
    float x, y, size, phase, attraction, attraction_x, attraction_y;
    uint32_t color_index;
    uint8_t kind, life_fraction; /* 0 expired, 255 full; vacuum locks lifetime */
    uint16_t reserved;
} snakes_core_food;
typedef struct snakes_core_item {
    uint64_t id;
    float x, y;
    uint8_t kind, reserved_byte;
    uint16_t age_ticks, life_ticks, reserved;
} snakes_core_item;
typedef struct snakes_core_event {
    uint64_t tick;
    float x, y;
    uint32_t snake_id, other_snake_id, color_index;
    uint8_t kind, reserved[3];
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
/* Latest tick's event ring, oldest first, at most MAX_EVENTS. Reading does not
 * consume it; step(world,n) retains only the last tick. Overflow evicts oldest.
 * Items are reserved and always empty in R1. Failure changes no output. */
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
 * Dead/unplanned/scripted slots have zero counts. Invalid IDs return INVALID. */
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
SNAKES_CORE_ASSERT(sizeof(snakes_core_steering_input) == 24);
SNAKES_CORE_ASSERT(sizeof(snakes_core_snake) == 56);
SNAKES_CORE_ASSERT(sizeof(snakes_core_segment) == 16);
SNAKES_CORE_ASSERT(sizeof(snakes_core_food) == 48);
SNAKES_CORE_ASSERT(sizeof(snakes_core_frame_sizes) == 24);
SNAKES_CORE_ASSERT(sizeof(snakes_core_frame_info) == 40);
SNAKES_CORE_ASSERT(sizeof(snakes_core_statistics) == 56);
SNAKES_CORE_ASSERT(offsetof(snakes_core_snake, radius) == 16);
SNAKES_CORE_ASSERT(offsetof(snakes_core_food, color_index) == 36);
SNAKES_CORE_ASSERT(offsetof(snakes_core_statistics, deaths) == 16);
SNAKES_CORE_ASSERT(sizeof(snakes_core_item) == 24);
SNAKES_CORE_ASSERT(sizeof(snakes_core_event) == 32);
SNAKES_CORE_ASSERT(offsetof(snakes_core_config, rule_set) == 72);
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

/* SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef SNAKES_CORE_H
#define SNAKES_CORE_H
#include <stddef.h>
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
#define SNAKES_CORE_ABI_VERSION 1u
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
} snakes_core_config;
typedef struct snakes_core_steering_input {
    uint32_t id, generation;
    double desired_angle, rush; /* rush in [0,1]; generation 0 matches any */
} snakes_core_steering_input;
typedef struct snakes_core_snake {
    uint32_t id, generation, alive, color_index;
    double radius, angle, desired_angle;
    uint32_t segment_offset, segment_count;
} snakes_core_snake;
typedef struct snakes_core_segment { float x, y, previous_x, previous_y; } snakes_core_segment;
typedef struct snakes_core_food {
    uint64_t id;
    float x, y, size, phase, attraction, attraction_x, attraction_y;
    uint32_t color_index;
} snakes_core_food;
typedef struct snakes_core_frame_sizes { uint32_t snakes, segments, food, reserved; } snakes_core_frame_sizes;
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
/* Count or seed changes restart the world. Other changes preserve state. */
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
int32_t snakes_core_stats(const snakes_core_world *world, snakes_core_statistics *output);
/* New optional AI overlay API. Empty steering tables now restore the smart
 * default AI. Existing ABI records and ABI version remain unchanged. */
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
 * hazards and reused until the next strategy decision. */
int32_t snakes_core_ai_debug(const snakes_core_world *world, uint32_t id,
                           snakes_core_ai_debug_record *output);
#ifdef __cplusplus
}
#endif
#if defined(__cplusplus)
#define SNAKES_CORE_ASSERT(c) static_assert(c, #c)
#else
#define SNAKES_CORE_ASSERT(c) _Static_assert(c, #c)
#endif
SNAKES_CORE_ASSERT(sizeof(snakes_core_config) == 72);
SNAKES_CORE_ASSERT(sizeof(snakes_core_steering_input) == 24);
SNAKES_CORE_ASSERT(sizeof(snakes_core_snake) == 48);
SNAKES_CORE_ASSERT(sizeof(snakes_core_segment) == 16);
SNAKES_CORE_ASSERT(sizeof(snakes_core_food) == 40);
SNAKES_CORE_ASSERT(sizeof(snakes_core_frame_sizes) == 16);
SNAKES_CORE_ASSERT(sizeof(snakes_core_frame_info) == 40);
SNAKES_CORE_ASSERT(sizeof(snakes_core_statistics) == 56);
SNAKES_CORE_ASSERT(offsetof(snakes_core_snake, radius) == 16);
SNAKES_CORE_ASSERT(offsetof(snakes_core_food, color_index) == 36);
SNAKES_CORE_ASSERT(offsetof(snakes_core_statistics, deaths) == 16);
SNAKES_CORE_ASSERT(sizeof(snakes_core_ai_debug_point) == 8);
SNAKES_CORE_ASSERT(sizeof(snakes_core_ai_debug_record) == 200);
SNAKES_CORE_ASSERT(offsetof(snakes_core_ai_debug_record, target_food_ids) == 32);
SNAKES_CORE_ASSERT(offsetof(snakes_core_ai_debug_record, path) == 72);
#undef SNAKES_CORE_ASSERT
#endif

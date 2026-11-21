/* SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef VISUALS_DEPTH_H
#define VISUALS_DEPTH_H
#include <stddef.h>
#include <stdint.h>
#ifdef __cplusplus
extern "C"
{
#endif
    /* Internal ABI: one binary, frozen strides, no live mutable reconfiguration. */
    typedef struct visuals_depth_world visuals_depth_world;
    typedef struct visuals_depth_sample visuals_depth_sample;
    typedef struct
    {
        double x, y, width, height;
    } vd_rect;
    typedef struct
    {
        uint32_t id, kind;
        const char* key;
        double minimum, maximum, default_value;
        const char* choices;
    } vd_parameter;
    typedef struct
    {
        uint32_t id, kind;
        uint64_t payload;
    } vd_value;
    typedef struct
    {
        uint32_t visual, palette;
        uint64_t seed;
        double speed, density, scale, trails;
        uint32_t reduced_motion, monitor_mode;
    } vd_config;
    typedef struct
    {
        uint64_t id;
        vd_rect viewport;
        double dpr, refresh_hz;
        uint32_t primary, padding;
    } vd_output;
    typedef struct
    {
        uint32_t background, frame_cap;
        float top[4], bottom[4];
    } vd_environment;
    typedef struct
    {
        uint64_t output_id;
        vd_rect rect;
        double inflation;
        uint32_t kind, enabled;
    } vd_obstacle;
    typedef struct
    {
        uint64_t output_id, layout_generation;
        int64_t timeline_ns;
        vd_rect viewport;
        double uniform_scale, offset_x, offset_y, dpr;
    } vd_view;
    typedef struct
    {
        uint64_t layout, topology, data, sample;
    } vd_generations;
    typedef struct
    {
        uint64_t id, generation;
        double x, y;
        float radius, rgba[4];
        uint32_t flags;
        int64_t birth_ns, death_ns;
    } vd_circle;
    typedef struct
    {
        vd_view view;
        vd_rect world_bounds;
        vd_generations generations;
        uint64_t lower_tick, upper_tick;
        double alpha;
        uint32_t history_miss, padding;
    } vd_sample_info;
    typedef struct
    {
        const vd_circle* circles;
        size_t circle_count;
        uint64_t texture_revision;
        uint32_t texture_width, texture_height;
        const uint8_t* texture_bytes;
        size_t texture_size;
        const uint8_t* payload;
        size_t payload_size;
        double emission;
    } vd_endpoint;
    typedef struct
    {
        uint64_t ticks, history_misses, p99_ns, max_ns, snapshot_bytes;
    } vd_stats;
    int32_t visuals_depth_schema(uint32_t, const vd_parameter**, size_t*);
    uint32_t visuals_depth_available(uint32_t);
    uint32_t visuals_depth_mirrored_policy(uint32_t);
uint32_t visuals_depth_mirrored_fit(uint32_t);
    int32_t visuals_depth_create(const vd_config*, const vd_value*, size_t, const vd_output*, size_t,
        const vd_environment*, const vd_obstacle*, size_t, uint64_t, int64_t, visuals_depth_world**);
    void visuals_depth_destroy(visuals_depth_world*);
    /* 0 ready, 1 more bounded work required, -1 invalid. Max 1024 ticks per call. */
    int32_t visuals_depth_ensure_through(visuals_depth_world*, int64_t, uint32_t);
    int32_t visuals_depth_retain_sample(visuals_depth_world*, const vd_view*, visuals_depth_sample**);
    void visuals_depth_sample_release(visuals_depth_sample*);
    int32_t visuals_depth_sample_info(const visuals_depth_sample*, vd_sample_info*);
    int32_t visuals_depth_sample_endpoint(const visuals_depth_sample*, uint32_t, vd_endpoint*);
    int32_t visuals_depth_stats(const visuals_depth_world*, vd_stats*);
#ifdef __cplusplus
}
static_assert(sizeof(vd_config) == 56 && sizeof(vd_output) == 64 && sizeof(vd_view) == 88);
static_assert(sizeof(vd_circle) == 72 && offsetof(vd_circle, birth_ns) == 56);
static_assert(sizeof(vd_environment) == 40 && sizeof(vd_obstacle) == 56 && sizeof(vd_value) == 16);
static_assert(sizeof(vd_stats) == 40 && sizeof(vd_generations) == 32 && offsetof(vd_circle, death_ns) == 64);
static_assert(sizeof(vd_sample_info) == 184 && sizeof(vd_parameter) == 48 && sizeof(vd_endpoint) == 72);
#endif
#endif

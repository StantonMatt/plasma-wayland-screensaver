/* SPDX-License-Identifier: GPL-3.0-or-later */
#pragma once
#include "visuals_depth.h"
#include <stdbool.h>
#ifdef __cplusplus
extern "C" {
#endif
enum { VD_RIBBON_NODES = 10754, VD_RIBBON_DRAWS = 82 };
typedef struct { float position[2], normal[2], half_width, facing, pulse, alpha;
#ifdef __cplusplus
    float halo_radius = 0, cap_mix = 0, tip_distance = 0, tip_radius = 0;
#else
    float halo_radius, cap_mix, tip_distance, tip_radius;
#endif
 } vd_ribbon_node;
typedef struct { vd_ribbon_node lower, upper; float tag; } vd_ribbon_pair;
typedef struct { float front[4], back[4], halo[4], accent[4]; } vd_ribbon_silk;
typedef struct {
    uint32_t first, count, motes, composition;
    float opacity;
    uint32_t material;
    vd_ribbon_silk silk;
#ifdef __cplusplus
    uint32_t strand = 0;
    float shoulder_gain[2] = {};
#else
    uint32_t strand;
    float shoulder_gain[2];
#endif
} vd_ribbon_draw;
typedef struct { float position[2], normal[2], silk[4]; } vd_ribbon_body;
typedef struct { vd_ribbon_body lower, upper; uint32_t tag; } vd_ribbon_motion;
typedef struct { float lower[4], upper[4]; } vd_ribbon_cap;
size_t visuals_ribbons_write_packed(const visuals_depth_sample*, vd_ribbon_motion*, size_t,
    vd_ribbon_cap*, size_t, size_t*, vd_ribbon_draw*, size_t, size_t*);
float visuals_ribbons_dissolve(const visuals_depth_sample*);
bool visuals_ribbons_same_pair(const visuals_depth_sample*, const visuals_depth_sample*);
size_t visuals_ribbons_write(const visuals_depth_sample*, vd_ribbon_pair*, size_t node_capacity,
    vd_ribbon_draw*, size_t draw_capacity, size_t* node_count);
#ifdef __cplusplus
}
static_assert(sizeof(vd_ribbon_motion) == 68 && sizeof(vd_ribbon_cap) == 32);
static_assert(sizeof(vd_ribbon_node) == 48 && sizeof(vd_ribbon_pair) == 100 && sizeof(vd_ribbon_draw) == 100);
#endif

/* SPDX-License-Identifier: GPL-3.0-or-later */
#pragma once
#include "visuals_depth.h"
#include <stdbool.h>
#ifdef __cplusplus
extern "C" {
#endif
typedef struct {
    const float (*texels)[4];
    uint32_t length;
} vd_firefly_row;
typedef struct {
    vd_firefly_row lower, upper, grass_lower, grass, blades, terrain, history[61];
    uint64_t lower_serial, upper_serial, history_serial;
    uint32_t mid_count, trail_segments, ring_length, still;
    /* colours[0].a identifies the lantern palette; RGB is reflected in the grass.
     * bottom stores the ground RGB and the world flight-speed multiplier. */
    float colours[8][4], bottom[4], art[4];
} vd_firefly_frame;
/* Every pointer remains readable while the corresponding sample is retained. */
bool visuals_fireflies_frame(const visuals_depth_sample*, vd_firefly_frame*);
#ifdef __cplusplus
}
static_assert(sizeof(float[4]) == 16);
#endif

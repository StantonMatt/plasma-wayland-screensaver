/* SPDX-License-Identifier: GPL-3.0-or-later */
#pragma once
#include "visuals_depth.h"
#include <stdbool.h>
#ifdef __cplusplus
extern "C" {
#endif
typedef struct {
    float geom[4], axis[4], colour[4], look[4];
} vd_bounce_record;
typedef struct {
    uint64_t serial;
    const float (*knots)[8];
} vd_bounce_ring;
typedef struct {
    uint32_t count, trail_start, sphere_start, trails;
    float trail_half_width;
    float top[4], bottom[4];
    float trail_width_scale; /* consumes existing padding; ring offset stays 56 */
    vd_bounce_ring rings[61];
} vd_bounce_batch;
// Ring data belongs to the retained sample. Retry with count slots on overflow.
bool visuals_depth_bounce_write(const visuals_depth_sample*, vd_bounce_record*, size_t, vd_bounce_batch*);
#ifdef __cplusplus
}
static_assert(offsetof(vd_bounce_batch, rings) == 56);
static_assert(sizeof(vd_bounce_record) == 64 && sizeof(vd_bounce_ring) == 16);
#endif

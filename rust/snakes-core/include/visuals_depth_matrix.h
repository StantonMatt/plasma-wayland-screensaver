/* SPDX-License-Identifier: GPL-3.0-or-later */
#pragma once
#include "visuals_depth.h"
#include <stdbool.h>
#ifdef __cplusplus
extern "C"
{
#endif
    typedef struct vd_rain_still vd_rain_still;
    typedef struct
    {
        uint32_t stamp;
        uint16_t lo, hi;
    } vd_rain_dirty;
    typedef struct
    {
        float viewport[4], grid[4], section[4], light[4], weather[4], background[4], colors[6][4];
    } vd_rain_uniforms;
    typedef struct
    {
        const uint8_t* cells;
        const vd_rain_dirty* dirty;
        uint32_t rows, col_first, col_count, row_first, row_count, stamp, still;
        vd_rain_uniforms uniforms;
    } vd_rain_layer;
    typedef struct
    {
        float path[4], ink[4], light[4];
    } vd_rain_head;
    bool visuals_rain_layer(const visuals_depth_sample*, const vd_rain_still*, size_t, vd_rain_layer*);
    size_t visuals_rain_heads(const visuals_depth_sample*, const vd_rain_still*, vd_rain_head*, size_t);
    vd_rain_still* visuals_rain_still(const visuals_depth_sample*, uint64_t);
    void visuals_rain_release_still(vd_rain_still*);
#ifdef __cplusplus
}
static_assert(sizeof(vd_rain_head) == 48 && sizeof(vd_rain_dirty) == 8 && sizeof(vd_rain_uniforms) == 192);
#endif

/* SPDX-License-Identifier: GPL-3.0-or-later */
#pragma once
#include "visuals_depth.h"
#ifdef __cplusplus
extern "C" {
#endif
typedef struct {
    float left[4], right[4], field[4], material[4], tangent_left[4], tangent_right[4], optics[4];
} aurora_quad;
/* phases: independently wrapped Spectrum hue (radians), dither frame, corona envelope, reserved. */
typedef struct {
    float bounds[4], colors[6][4], top[4], bottom[4], style[4], clock[4], view[4], optics[4], overlap[8][4], phases[4];
} aurora_atmosphere;
/* Negative for live motion, otherwise the incoming still's opacity in [0, 1].
   The side argument below is -1 for interpolation, 0 for lower, 1 for upper. */
float visuals_aurora_dissolve(const visuals_depth_sample*);
/* Returns the requested target scale, or zero for an incompatible lease. */
float visuals_aurora_atmosphere(const visuals_depth_sample*, int32_t side, aurora_atmosphere*);
const uint8_t* visuals_aurora_horizon(const visuals_depth_sample*, int32_t side, uint32_t* width);
/* Curtain vertices carry xy, the slow material coordinate and hem slope.
   Their light vector carries ray coordinate, intensity, signed normal distance
   and storm excitation. Other primitives use position_uv.zw as ordinary UV. */
typedef struct {
    float position_uv[4], light[4], surface[4];
} aurora_vertex;
/* Shared-edge triangle strips, with degenerate separators between independent sheets. */
size_t visuals_aurora_strip(const visuals_depth_sample*, uint32_t layer, int32_t side, aurora_vertex*, size_t capacity);
/* Conservative initial allocation before per-view clipping. */
size_t visuals_aurora_quad_capacity(const visuals_depth_sample*, uint32_t layer, int32_t side);
size_t visuals_aurora_quads(const visuals_depth_sample*, uint32_t layer, int32_t side, aurora_quad*, size_t capacity);
#ifdef __cplusplus
}
static_assert(sizeof(aurora_vertex) == 48 && sizeof(aurora_quad) == 112 && sizeof(aurora_atmosphere) == 352);
#endif

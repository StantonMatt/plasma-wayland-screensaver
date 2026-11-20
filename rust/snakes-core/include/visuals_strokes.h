// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef VISUALS_STROKES_H
#define VISUALS_STROKES_H
#include "visuals_core.h"
#ifdef __cplusplus
extern "C" {
#endif
// Additive ABI extension. Existing visuals_core v1 and Snakes ABI stay unchanged.
#define VISUALS_STROKES_ABI_VERSION 1u
#define VISUALS_CORE_RIBBONS 4u
#define VISUALS_CORE_KALEIDOSCOPE 5u
typedef struct visuals_strokes visuals_strokes;
// Frozen v1 vertices; triangles, in original draw order, logical item coordinates.
// Stroke: shape=(signed lateral distance, butt-cap distance,0,0),
// parameters=(half width,-1). Circle pair: shape=(dx1,dy1,dx2,dy2),
// parameters=(r1,r2). Color is straight (unpremultiplied) RGBA; the shader
// applies coverage and premultiplies once before additive RGB+alpha blending.
// Union maximum coverage within each stroke before additive blending between
// strokes. Draw into a transparent RGBA8 layer, then source-over that layer.
typedef struct {
    float position[2], shape[4], parameters[2], color[4];
} visuals_strokes_vertex;
uint32_t visuals_strokes_abi_version(void);
// Uses frozen visuals_core_config/request v1 layouts. Valid visual IDs are 4/5.
// Single-threaded opaque handle; no caller pointers are retained. Null/error on
// invalid records. Destroy exactly once (null accepted).
visuals_strokes *visuals_strokes_create(const visuals_core_config *config);
int32_t visuals_strokes_configure(visuals_strokes *,const visuals_core_config *);
void visuals_strokes_destroy(visuals_strokes *);
size_t visuals_strokes_vertex_count(const visuals_strokes *,const visuals_core_request *);
// Output buffers must be aligned, exclusively writable and fully initialised
// (all capacity vertices / bytes respectively) before either output call: Rust
// borrows their storage as mutable slices. Input/output buffers must not overlap.
// capacity is vertices; returns vertex count or -1. No writes/phase changes on failure.
int32_t visuals_strokes_frame(visuals_strokes *,const visuals_core_request *,visuals_strokes_vertex *,size_t capacity);
// Explicit software/pipeline-rejection fallback. Output is premultiplied RGBA8.
int32_t visuals_strokes_raster(const visuals_strokes_vertex *,size_t count,double logical_width,double logical_height,
    uint32_t width,uint32_t height,uint8_t *rgba,size_t capacity);
#ifdef __cplusplus
}
static_assert(sizeof(visuals_strokes_vertex)==48);
static_assert(offsetof(visuals_strokes_vertex,shape)==8);
static_assert(offsetof(visuals_strokes_vertex,parameters)==24);
static_assert(offsetof(visuals_strokes_vertex,color)==32);
#endif
#endif

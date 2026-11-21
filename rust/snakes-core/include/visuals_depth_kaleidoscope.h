/* SPDX-License-Identifier: GPL-3.0-or-later */
#pragma once
#include "visuals_depth.h"
#ifdef __cplusplus
extern "C" {
#endif
typedef struct {
    float vertices[6][2], shape[4], material[4];
} kaleidoscope_template;
typedef struct {
    float pose[4];
} kaleidoscope_pose;
typedef struct {
    float eye[4], motion[4], glass[4], light[4], flare[4], colors[6][4], rim[4], guide[4], mote[4];
} kaleidoscope_lens;
/* Templates borrow the lease through its upper endpoint. */
const kaleidoscope_template* visuals_depth_kaleidoscope_templates(const visuals_depth_sample*, size_t* count);
size_t visuals_depth_kaleidoscope_sample(
    const visuals_depth_sample*, kaleidoscope_lens*, kaleidoscope_pose*, size_t capacity);
#ifdef __cplusplus
}
static_assert(
    sizeof(kaleidoscope_template) == 80 && sizeof(kaleidoscope_pose) == 16 && sizeof(kaleidoscope_lens) == 224);
#endif

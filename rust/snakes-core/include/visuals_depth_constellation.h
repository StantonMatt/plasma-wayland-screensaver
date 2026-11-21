/* SPDX-License-Identifier: GPL-3.0-or-later */
#pragma once
#include "visuals_core.h"
#include "visuals_depth.h"
#include <stdbool.h>
#ifdef __cplusplus
extern "C" {
#endif
size_t visuals_constellation_primitives(
    const visuals_depth_sample*, bool static_layer, visuals_core_primitive*, size_t capacity);
void visuals_constellation_parallax(const visuals_depth_sample*, float offset[2]);
#ifdef __cplusplus
}
static_assert(sizeof(visuals_core_primitive) == 48);
#endif

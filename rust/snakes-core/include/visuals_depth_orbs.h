/* SPDX-License-Identifier: GPL-3.0-or-later */
#pragma once
#include "visuals_depth.h"
#include <stdbool.h>
#ifdef __cplusplus
extern "C" {
#endif
typedef struct {
    float body[4], lobe[4], surface[4];
} vd_orb_record;
typedef struct {
    float look, glow, phase, blend, palette;
} vd_orb_style;
/* Each row contains the two immutable tick endpoints. */
size_t visuals_orbs_records(const visuals_depth_sample*, vd_orb_record*, size_t row_capacity);
bool visuals_orbs_style(const visuals_depth_sample*, vd_orb_style*);
bool visuals_orbs_clock(const visuals_depth_sample*, double position[2]);
#ifdef __cplusplus
}
static_assert(sizeof(vd_orb_record) == 48 && sizeof(vd_orb_style) == 20);
#endif

/* SPDX-License-Identifier: GPL-3.0-or-later */
#pragma once
#include "visuals_depth.h"
#include "visuals_core.h"
#include <stdbool.h>
#ifdef __cplusplus
extern "C" {
#endif
enum { VD_STAR_ROWS = 250, VD_STAR_WIDTH = 1024, VD_STAR_SPRITES = 16 };
typedef struct {
    float body[4], light[4];
} vd_star;
typedef struct {
    float shape[4], material[4], detail[4];
} vd_sky_sprite;
typedef struct {
    float projection[4], viewport[4], rotation[4], old_rotation[4];
    float eye[2][4], shift[4], optics[4], surface[4], field[4], axis[4], old_axis[4], destination[4], palette[6][4];
} vd_star_optics;
typedef struct {
    const vd_star* stars;
    uint64_t stamp;
    uint32_t row, count;
} vd_star_upload;
typedef struct {
    vd_star_optics optics;
    uint32_t groups[512];
    vd_sky_sprite sprites[VD_STAR_SPRITES];
    uint32_t group_count, sprite_count, updates, debris;
    uint32_t row_capacity; /* padded stars in one physical texture row */
    uint32_t near_group_count; /* first groups in the buffer, drawn in front of dust */
    vd_star_upload upload[VD_STAR_ROWS];
} vd_star_packet;
/* Row pointers borrow the retained lease. Only missing or replaced visible
   cells are listed, including when a slow output skips several stream revisions. */
bool visuals_starfield_sample(const visuals_depth_sample*, const uint64_t resident[VD_STAR_ROWS], vd_star_packet*);
uint32_t visuals_starfield_software_material(visuals_core_primitive*, uint32_t count, uint32_t palette);
const uint8_t* visuals_starfield_noise(void); /* immutable 256 x 256 RGBA8 */
#ifdef __cplusplus
}
static_assert(sizeof(vd_star) == 32 && sizeof(vd_star_optics) == 304 && sizeof(vd_sky_sprite) == 48);
static_assert(offsetof(vd_star_packet, upload) == 3144);
#endif

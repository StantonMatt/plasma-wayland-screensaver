/* SPDX-License-Identifier: GPL-3.0-or-later */
#pragma once
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
/* Two separate, tightly packed size x size buffers. Positive distance is ink.
 * The 0.5 contour is the glyph edge; spread is measured in source pixels. */
bool visuals_glyph_sdf(const uint8_t *coverage, uint8_t *distance, size_t size, float spread);
#ifdef __cplusplus
}
#endif

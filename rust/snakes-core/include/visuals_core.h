// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef VISUALS_CORE_H
#define VISUALS_CORE_H
#include <stdint.h>
#include <stddef.h>
#ifdef __cplusplus
extern "C" {
#endif
#define VISUALS_CORE_ABI_VERSION 1u
#define VISUALS_CORE_AURORA 1u
#define VISUALS_CORE_MAX_BANDS 9u
// Palette IDs: ocean=0, spectrum=1, ember=2, forest=3, mono=4, pastel=5.
// v1 records are frozen. Incompatible layouts require a new ABI version.
typedef struct visuals_core visuals_core;
typedef struct {
    uint32_t abi_version, struct_size, visual, palette;
    double density, scale, speed, trails;
    int32_t seed;
    uint32_t reduced_motion;
    int64_t epoch_ns;
} visuals_core_config;
typedef struct { float geometry[4], inner[4], middle[4]; } visuals_core_band;
typedef struct {
    visuals_core_band bands[VISUALS_CORE_MAX_BANDS];
    float shade[4], viewport[2];
    uint32_t band_count, reserved;
} visuals_core_snapshot;
typedef struct {
    double width, height;
    int64_t presentation_ns;
    uint32_t advance, reserved;
    // x/y/width/height, then viewport x/y in global world coordinates.
    // Negative origins/gaps/insets are valid. Aurora retains local mapping.
    double world_bounds[4], viewport_origin[2];
} visuals_core_request;
uint32_t visuals_core_abi_version(void);
// Null/error returns on invalid records. No caller pointers are retained.
visuals_core *visuals_core_create(const visuals_core_config *config);
int32_t visuals_core_configure(visuals_core *core, const visuals_core_config *config);
void visuals_core_destroy(visuals_core *core);
// Single-threaded handle. Immutable caller-owned snapshots may cross threads.
// capacity is bytes. 0 = success, -1 = invalid arguments/capacity.
int32_t visuals_core_frame(visuals_core *core, const visuals_core_request *request,
                          visuals_core_snapshot *out, size_t capacity);
int32_t visuals_core_raster(const visuals_core_snapshot *frame, uint32_t width,
                           uint32_t height, uint8_t *rgba, size_t capacity);
#ifdef __cplusplus
}
static_assert(sizeof(visuals_core_config) == 64);
static_assert(alignof(visuals_core_config) == 8);
static_assert(offsetof(visuals_core_config, density) == 16);
static_assert(offsetof(visuals_core_config, epoch_ns) == 56);
static_assert(sizeof(visuals_core_band) == 48);
static_assert(offsetof(visuals_core_band, inner) == 16);
static_assert(offsetof(visuals_core_band, middle) == 32);
static_assert(sizeof(visuals_core_snapshot) == 464);
static_assert(offsetof(visuals_core_snapshot, shade) == 432);
static_assert(offsetof(visuals_core_snapshot, band_count) == 456);
static_assert(sizeof(visuals_core_request) == 80);
static_assert(offsetof(visuals_core_request, world_bounds) == 32);
#endif
#endif

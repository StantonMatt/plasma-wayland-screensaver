// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef VISUALS_BOUNCE_H
#define VISUALS_BOUNCE_H
#include "visuals_core.h"
#ifdef __cplusplus
extern "C" {
#endif
#define VISUALS_BOUNCE_ABI_VERSION 1u
#define VISUALS_BOUNCE_VISUAL 7u
#define VISUALS_BOUNCE_MAX_RECORDS 180u
// This extension preserves visuals_core ABI v2 and its 48-byte primitives.
// Kinds 3/4/5: two-centre gradient ball / bordered flat ball / scaled trail disc.
// For these new kinds, geometry.zw are local-to-viewport scale factors.
// ABI v2 kinds 1/2 retain their original meaning: geometry.zw are ignored.
typedef struct visuals_bounce visuals_bounce;
typedef struct {
    uint32_t abi_version, struct_size, count, mode; // 0 independent, 1 synchronized, 2 seamless
    double gravity, elasticity; // user settings (-100..100, 50..100)
    uint32_t collisions, reserved;
} visuals_bounce_settings;
typedef struct { double x, y, vx, vy, size; } visuals_bounce_ball;
visuals_bounce *visuals_bounce_create(const visuals_core_config *, const visuals_bounce_settings *);
int32_t visuals_bounce_configure(visuals_bounce *, const visuals_core_config *, const visuals_bounce_settings *);
void visuals_bounce_destroy(visuals_bounce *);
// Shared input is sampled from AnimationState's retained history, never retained.
// Independent mode ignores shared input and advances fixed-step Rust physics.
// advance=0 holds physics, including reduced motion. Capacity is records.
int32_t visuals_bounce_frame(visuals_bounce *, const visuals_core_request *, double delta_seconds,
    const visuals_bounce_ball *shared, size_t shared_count,
    visuals_core_primitive *, size_t capacity, uint32_t *count);
#ifdef __cplusplus
}
static_assert(sizeof(visuals_bounce_settings)==40);
static_assert(sizeof(visuals_bounce_ball)==40);
#endif
#endif

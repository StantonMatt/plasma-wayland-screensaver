// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef VISUALS_MATRIX_H
#define VISUALS_MATRIX_H
#include "visuals_core.h"
#ifdef __cplusplus
extern "C" {
#endif
#define VISUALS_MATRIX_ABI_VERSION 1u
#define VISUALS_MATRIX_VISUAL 6u
#define VISUALS_MATRIX_GLYPHS 37u
#define VISUALS_MATRIX_MAX_ROWS 30u
// Separate extension ABI; existing visuals_core v2 layouts/symbols stay unchanged.
typedef struct visuals_matrix visuals_matrix;
typedef struct {
    double column_width, line_height;
    uint32_t glyph_size, column_count;
    float stream_opacity, head_opacity;
} visuals_matrix_layout;
typedef struct {
    double x, fall_rate, start_offset, content_height, head_y;
    float color[4];
    uint32_t length;
    uint8_t glyphs[VISUALS_MATRIX_MAX_ROWS];
    uint8_t reserved[6];
} visuals_matrix_column;
uint32_t visuals_matrix_abi_version(void);
// config uses VISUALS_CORE_ABI_VERSION and visual=VISUALS_MATRIX_VISUAL.
visuals_matrix *visuals_matrix_create(const visuals_core_config *);
int32_t visuals_matrix_configure(visuals_matrix *, const visuals_core_config *);
void visuals_matrix_destroy(visuals_matrix *);
// Layout/settings work only. The handle caches column parameters; no retained caller memory.
int32_t visuals_matrix_get_layout(const visuals_matrix *, double width, visuals_matrix_layout *);
int32_t visuals_matrix_columns(visuals_matrix *, double width, double font_height,
    visuals_matrix_column *, size_t capacity);
// columns() font_height is ceil(QFontMetricsF::height()). Call it before frame().
// One RGBA32F texel/column: y offset, 0, 0, 0.
// Like the frozen QML, contentHeight includes integer-truncated lineHeightOffset.
// Frame calls allocate nothing, including the first call. Errors leave output untouched.
int32_t visuals_matrix_frame(visuals_matrix *, const visuals_core_request *,
    float *rgba_offsets, size_t capacity_columns);
#ifdef __cplusplus
}
static_assert(sizeof(visuals_matrix_layout)==32);
static_assert(sizeof(visuals_matrix_column)==96);
static_assert(offsetof(visuals_matrix_column,glyphs)==60);
#endif
#endif

/* SPDX-License-Identifier: GPL-3.0-or-later */
#include "snakes_core.h"
#include <assert.h>
#include <math.h>
int main(void) {
    snakes_core_config c = {1280, 720, 100, 100, 100, 100, 100, 73, 7, 1, 1, SNAKES_CORE_RULE_DEFAULT, 0};
    snakes_core_world *w = NULL;
    assert(snakes_core_create(&c, &w) == SNAKES_CORE_OK);
    assert(snakes_core_step(w, 1) == SNAKES_CORE_OK);
    snakes_core_ai_debug_record d = {0};
    assert(snakes_core_ai_debug(w, 0, &d) == SNAKES_CORE_OK);
    assert(d.path_count > 0 && d.path_count <= 16 && d.target_count <= 5);
    for (uint32_t i = 0; i < d.path_count; ++i) {
        assert(isfinite(d.path[i].x) && isfinite(d.path[i].y));
    }
    assert(snakes_core_ai_debug(w, 14, &d) == SNAKES_CORE_INVALID_ARGUMENT);
    snakes_core_steering_input input = {0, 0, 0.4, 0};
    assert(snakes_core_set_steering(w, &input, 1) == SNAKES_CORE_OK);
    assert(snakes_core_ai_debug(w, 0, &d) == SNAKES_CORE_OK);
    assert(d.path_count == 0 && d.target_count == 0);
    snakes_core_destroy(w);
    return 0;
}

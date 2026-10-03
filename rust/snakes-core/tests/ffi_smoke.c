/* SPDX-License-Identifier: GPL-3.0-or-later */
#include "snakes_core.h"
#include <assert.h>
#include <stdlib.h>
int main(void) {
    assert(snakes_core_abi_version() == SNAKES_CORE_ABI_VERSION);
    snakes_core_config config = {1280,720,50,35,100,100,75,73,7,0,1,SNAKES_CORE_RULE_DEFAULT,{0}};
    snakes_core_world *world = NULL;
    assert(snakes_core_create(&config, &world) == SNAKES_CORE_OK);
    snakes_core_frame_sizes sizes = {0};
    assert(snakes_core_get_frame_sizes(world, &sizes) == SNAKES_CORE_OK);
    snakes_core_snake *snakes = calloc(sizes.snakes, sizeof(*snakes));
    snakes_core_segment *segments = calloc(sizes.segments, sizeof(*segments));
    snakes_core_food *food = calloc(sizes.food, sizeof(*food));
    assert(snakes && segments && food);
    snakes_core_frame_info info = {0};
    assert(snakes_core_export_frame(world, snakes, sizes.snakes, segments,
        sizes.segments, food, sizes.food, &info) == SNAKES_CORE_OK);
    assert(info.tick == 0 && info.world_width == 1280);
    assert(snakes[0].generation == 1 && snakes[0].alive == 1);
    assert(segments[0].x == segments[0].previous_x);
    assert(snakes_core_step(world, 1) == SNAKES_CORE_OK);
    snakes_core_statistics stats = {0};
    assert(snakes_core_stats(world, &stats) == SNAKES_CORE_OK);
    assert(stats.alive <= sizes.snakes);
    snakes_core_destroy(world);
    free(snakes); free(segments); free(food);
    return 0;
}

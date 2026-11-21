/* Developer capture builds only. Never exposed by the installed production build. */
#pragma once
#include "visuals_depth.h"
#ifdef PVS_ENABLE_VISUAL_CAPTURE
#ifdef __cplusplus
extern "C"
{
#endif
    typedef struct
    {
        uint32_t id;
        int64_t at_ns;
        double anchor[2], strength;
    } vd_event;
    int32_t visuals_depth_debug_seed(visuals_depth_world*, uint64_t);
    int32_t visuals_depth_debug_step(visuals_depth_world*, uint32_t);
    int32_t visuals_depth_debug_seek(visuals_depth_world*, int64_t);
    int32_t visuals_depth_debug_inject(visuals_depth_world*, const vd_event*);
    uint64_t visuals_depth_debug_hash(const visuals_depth_world*);
#ifdef __cplusplus
}
#endif
#endif

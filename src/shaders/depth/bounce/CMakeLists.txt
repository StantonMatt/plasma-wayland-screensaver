qt_add_shaders(VisualDepth depth_bounce_shaders
    GLSL "300es,310es,320es,130,140,150,330" HLSL "50" MSL "12"
    PREFIX "/depth/bounce" BASE "${CMAKE_CURRENT_LIST_DIR}"
    FILES "${CMAKE_CURRENT_LIST_DIR}/bounce.vert" "${CMAKE_CURRENT_LIST_DIR}/bounce.frag")
